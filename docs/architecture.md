# Architecture

Stellar ROS Bridge sits between a ROS 2 robot graph and the Stellar network.
It is split into small crates so the domain logic stays pure and testable, and
so heavy external dependencies (ROS 2 and Stellar RPC) live only where needed.

## Crates

| Crate | Responsibility | External deps |
| --- | --- | --- |
| `bridge-core` | Domain types (`Capability`, `Task`, `TaskId`, `Proof`, `Reward`), traits (`Transport`, `Settlement`), error model. No I/O. | none |
| `ros-bridge` | Implements `Transport` using either rosbridge_suite JSON-over-WebSocket (default) or `rclrs` (`ros` feature). | `tungstenite`; `rclrs` + ROS 2 Jazzy when enabled |
| `stellar-bridge` | Implements `Settlement` against the TaskEscrow contract using Stellar RPC and Soroban transactions. | `soroban-client` 0.6 / `reqwest` / `stellar-xdr` |
| `bridge-node` | Runtime binary. Wires a `Transport` to a `Settlement`, runs the task lifecycle, serves health endpoints. | all of the above |
| `example-robot` | `TaskExecutor` implementation and robot-side demo: capability validation, telemetry digest, proof construction. | `sha2` |

The key design rule: **`bridge-core` never depends on ROS or Stellar.** Both
integrations are behind traits, which keeps the lifecycle unit-testable with
mocks and keeps changes scoped to one crate.

## Data flow

```
 robot                 ros-bridge              bridge-node              stellar-bridge        Stellar
   |  capability msg  ------>|                      |                          |                 |
   |                         |  Capability          |                          |                 |
   |                         |--------------------->|                          |                 |
   |                         |                      |  create/read task        |                 |
   |                         |                      |------------------------->|  TaskEscrow     |
   |  task request  <--------------------------------|                          |                 |
   |  (execute)  ----------->|  Task                |                          |                 |
   |                         |--------------------->|  accept_task             |                 |
   |                         |                      |------------------------->|  escrow funded  |
   |  proof (signed) ------->|  Proof               |                          |                 |
   |                         |--------------------->|  submit_proof/release    |                 |
   |                         |                      |------------------------->|  transfer       |
```

## ROS 2 transports

Both transports implement `bridge_core::Transport` and exchange
`std_msgs/msg/String` messages with a JSON `data` field. The payloads are the
serde encodings of `Capability`, `Task`, and `Proof`.

### rosbridge (default, portable)

`rosbridge::RosbridgeTransport` connects to a rosbridge_suite WebSocket server
(`ws://host:port`) and:

1. sends `{"op":"subscribe","topic":"<tasks>","type":"std_msgs/msg/String"}`;
2. sends `{"op":"advertise",...}` plus a capability publish when
   `advertise()` is called;
3. publishes proofs on the configured proofs topic;
4. polls the socket with a short read timeout in `next_task()`.

It compiles without ROS installed. Its frame construction and decoding are
unit-tested, and an in-process WebSocket test verifies the subscribe, task
delivery, advertise, and proof-publish round trip. A real ROS 2 rosbridge
deployment is not part of the test suite.

### rclrs (`ros` feature)

`rclrs_transport::RclrsTransport` creates a native node and uses rclrs dynamic
messages for `std_msgs/msg/String`. This avoids requiring generated Rust
message bindings at compile time: the type-support libraries are loaded from
the sourced ROS 2 installation at runtime. A background thread spins the
executor and callbacks push tasks into a queue consumed by `next_task()`.

This code is isolated behind `--features ros` and is built by the CI `ros` job
under ROS 2 Jazzy. It has **not** been compiled or run in this repository's
local Windows environment, where ROS 2 is unavailable.

### Task execution

A generic transport cannot know how to drive a particular robot, so both
transports take a `TaskExecutor`. `UnsupportedExecutor` is the default and
returns an explicit error. Applications implement `TaskExecutor` (or use
`FnExecutor`) to run the ROS action/service and return a `Proof`. This avoids
publishing a fake "completed" proof.

## Domain model

Implemented in `crates/bridge-core/src/types.rs`:

```rust
pub struct Capability {
    pub id: CapabilityId,    // e.g. "navigate", "pick"
    pub version: u32,
    pub units: String,       // e.g. "per_metre"
    pub metadata: serde_json::Value,
}

pub struct Task {
    pub id: TaskId,
    pub capability: CapabilityId,
    pub params: serde_json::Value,
    pub reward: Reward,      // asset + amount
    pub deadline: u64,       // Unix ledger time
    pub requester: AccountId,
}

pub struct Proof {
    pub task_id: TaskId,
    pub digest: [u8; 32],
    pub signer: AccountId,
    pub signature: [u8; 64],
}

pub enum TaskState {
    Announced,
    Requested,
    Accepted,
    InProgress,
    ProofSubmitted,
    Settled,
    Disputed,
    Expired,
}
```

`BridgeNode` drives `Announced -> Requested -> Accepted -> InProgress ->
ProofSubmitted -> Settled`, with an expiry path that refunds instead of
executing.

## Settlement contract (implemented)

`contracts/task-escrow` implements the escrow:

```rust
fn create_task(env, requester, capability, token, amount, deadline) -> Result<TaskId, Error>;
fn accept_task(env, task_id, executor) -> Result<(), Error>;
fn submit_proof(env, task_id, executor, digest) -> Result<(), Error>;
fn release(env, task_id) -> Result<(), Error>;
fn refund(env, task_id) -> Result<(), Error>;   // deadline passed
fn get_task(env, task_id) -> Option<Task>;
```

Safety invariants, covered by `contracts/task-escrow/src/test.rs`:

1. Funds leave escrow exactly once, via `release` **or** `refund`, never both.
2. Only the accepted executor can submit a proof, and only before `deadline`.
3. `release` requires a stored proof and requester authorization.
4. `refund` only succeeds after `deadline` and is requester-authorized.
5. State writes happen before token transfers, and persistent entries are
   TTL-bumped on every read/write.

Contract errors are a `#[contracterror]` enum with stable numeric codes 1-7.

## Live settlement client

`stellar-bridge::StellarSettlement` uses `soroban-client` 0.6 for:

- fetching the source account sequence number;
- building `InvokeHostFunction` transactions for each TaskEscrow entry point;
- simulating to obtain footprint, resource fees, and authorization entries;
- signing the envelope with the configured ed25519 secret;
- submitting and polling until success/failure.

The secret is moved into a `Keypair` at construction. `StellarConfig`'s `Debug`
implementation redacts it, and configuration/validation errors never echo it.

The client also contains pure, offline-tested code:

- `ContractCall` builds the exact argument list for every entry point.
- `parse_contract_error` maps `Error(Contract, #N)` to the contract enum.
- `parse_task_state` decodes `get_task`'s `Option<Task>`/`Status` from Soroban
  XDR into `TaskState` (`Open -> Requested`, `Released -> Settled`,
  `Refunded -> Expired`, and so on).
- `token_contract` selects the reward's SAC or the configured native XLM SAC.

### Authorization roles

The contract uses `require_auth` on the actor named by each entry point. One
`StellarSettlement` instance signs as one Stellar account, so deployments use:

- a requester client for `create_task`, `release`, and `refund`;
- an executor client for `accept_task` and `submit_proof`.

`submit_proof` also checks that the proof's `signer` matches the settlement
account before sending the transaction.

### Example robot

`crates/example-robot` supplies `SimulatedRobot`, a `TaskExecutor` that:

- rejects capabilities the robot did not advertise;
- hashes `task.id`, capability and params into a 32-byte SHA-256 digest;
- returns a proof for the executor identity (with an explicitly empty signature,
  so simulated proofs cannot be mistaken for production attestations).

`cargo run -p example-robot --bin simulated-robot` drives the complete
`BridgeNode` lifecycle with the simulated robot and prints the settled task and
proof digest. `crates/ros-bridge/examples/native_robot.rs` is the native ROS 2
counterpart: it advertises a capability and executes incoming tasks, and is
compiled under Jazzy in the `ros` CI job.

### Verified testnet deployment

The client was run end-to-end against testnet with two throwaway funded
accounts (a 1-stroop reward):

```
TaskEscrow:    CD55XYVX5AXAJNKBKALP6AESUNA2NZHTWOCNK3QUW4EYHGTN4LIHS46X
wasm sha256:   b9a5e4dbae0be3354d8679e57408092d9a395c510e7d65cc8f3dbc317fd11b70
wasm upload:   20be402246b17ce193ed0ac8adc21de6546bc7145d0c37b1d69e87811491b0ea
deploy:        fcc1a58bc1eb0244ad21698893cbd73ccf39b8f82e834a1dd623dd62313747a5
create_task:   032f2de1be35520521c88c48ceb1242b0da85883ff6815ca7cf336e354e88b51
release:       62a2d7ccc6a5873eaedddf161137c71c2faec34cddd3e4eb18f7d8269c838ca1
```

The smoke binary logged `Requested -> Accepted -> ProofSubmitted -> Settled`.
This is a manual test (`cargo run -p stellar-bridge --bin stellar-testnet-smoke`),
not part of `cargo test`; the CI contract job still runs the offline tests.

## Identity & keys

- Each robot maps to a Stellar account. The signing key lives in the operator's
  environment (never in the repo).
- ROS 2 node identity is separate from the Stellar account; the bridge holds the
  mapping. Linking the two is out of scope for the first milestones.
- Configuration is loaded at runtime from `StellarConfig::from_env()`; nothing
  secret is serialized into logs, docs, or test fixtures.

## Local development

1. Install Rust via rustup (toolchain pinned in `rust-toolchain.toml`).
2. Default builds need no ROS 2 installation.
3. For native ROS work: install ROS 2 Jazzy and source its setup file.
4. For Stellar work: create funded testnet accounts and set `STELLAR_SECRET`,
   `TASK_ESCROW_CONTRACT_ID`, and (for XLM) `STELLAR_TOKEN_CONTRACT_ID`.
5. `cargo build --workspace`, `cargo test --workspace`, and
   `cargo clippy --workspace --all-targets -- -D warnings` run offline.
6. `cargo run -p stellar-bridge --bin stellar-testnet-smoke` performs real
   network calls only when invoked manually.

### Windows GNU note

The bundled libsodium static library shipped by `stellar-baselib` expects the
C23 `memset_explicit` symbol, which the project's MinGW runtime does not
provide. `crates/stellar-bridge/build.rs` assembles a tiny x86_64 assembly shim
and links it only on `windows-gnu`. Other platforms are unaffected.

## Cross-cutting concerns

- **Observability:** structured `tracing` spans keyed by `TaskId`; the runtime
  health endpoints remain in `bridge-node`.
- **Time:** task deadlines are ledger time, not wall clock, to avoid skew.
- **Failure:** a failed settlement never blocks the robot loop; tasks are
  retried or refunded per the state machine.
- **Execution boundary:** payment settlement and physical execution are
  separate. The transport's `TaskExecutor` is the only place that claims a
  robot completed work.
