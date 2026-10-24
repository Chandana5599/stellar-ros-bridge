# Stellar ROS Bridge

> A Rust framework that connects [ROS 2](https://ros.org) robots to the
> [Stellar](https://stellar.org) network, letting machines advertise
> capabilities, accept paid tasks, and settle rewards on-chain via
> [Soroban](https://stellar.org/soroban) smart contracts.

[![CI](https://github.com/Primex-tech-labs/stellar-ros-bridge/actions/workflows/ci.yml/badge.svg)](https://github.com/Primex-tech-labs/stellar-ros-bridge/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

## Why this exists

Robots and autonomous fleets increasingly need to transact: rent out idle
capacity, pay for compute, settle for a delivered task, or coordinate across
organizations that do not trust one another. Today that coordination is glued
together with bespoke APIs, spreadsheets, and trust.

Stellar ROS Bridge closes that gap:

- **ROS 2** is where the robot's capabilities, sensors, and actions already live.
- **Stellar + Soroban** provide fast, low-cost, programmable settlement with a
  mature wallet and asset ecosystem.

This project is the seam between the two: a typed, testable, Rust-native bridge
that turns a robot's ROS 2 graph into a set of tradable on-chain services.

## Core ideas

| Concept | Description |
| --- | --- |
| **Capability** | A robot's advertised service (e.g. `navigate`, `pick`, `inspect`) published on a ROS 2 topic. |
| **Task** | A unit of paid work matching a Capability, with parameters, reward, and deadline. |
| **Escrow** | A Soroban contract that holds the reward until the task is proven complete. |
| **Proof** | Evidence a task finished (signed telemetry digest, sensor attestation, or maintainer sign-off). |
| **Settlement** | On-chain release of escrowed funds to the robot's Stellar account. |

## Architecture

```
            ROS 2 graph                         Stellar network
   +---------------------------+        +----------------------------+
   |  /robot/capabilities      |        |  TaskEscrow (Soroban)      |
   |  /robot/tasks (in)        |  <-->  |  Assets / XLM              |
   |  /robot/proofs (out)      |        +-------------+--------------+
   +-------------+-------------+                      |
                 |                                    |
                 |            stellar-ros-bridge       |
                 +------------------------------------+
                        bridge-node (runtime)
                   crates/bridge-core   (domain types)
                   crates/ros-bridge    (ROS transports)
                   crates/stellar-bridge (Soroban client)
```

See [`docs/architecture.md`](docs/architecture.md) for the full design and
[`docs/roadmap.md`](docs/roadmap.md) for the phased plan.

## Repository layout

```
crates/
  bridge-core/      # Domain types, state machine, driver, mocks (no I/O)
  ros-bridge/       # rosbridge WebSocket + rclrs transports
  stellar-bridge/   # Live Stellar RPC / TaskEscrow settlement client
  bridge-node/      # Binary: config, demo lifecycle, health server
contracts/
  task-escrow/      # Soroban escrow contract + tests (own workspace)
docs/               # Architecture, roadmap, contributor guides
.wave/              # Drips Wave issue backlog and process notes
```

## Status

**Working vertical slice, with live adapters implemented.**

Locally implemented and tested without ROS or a live network:

- `bridge-core`: validated domain types, the task state machine, `Transport` /
  `Settlement` / `Clock` traits, mocks, and the `BridgeNode` driver
  (accept -> execute -> prove -> release/refund).
- `contracts/task-escrow`: a Soroban escrow with requester/executor auth,
  deadlines, proof digests, TTL bumps, release/refund and unit tests.
- `bridge-node --demo`: runs the full lifecycle with in-memory collaborators.
- `example-robot`: a `TaskExecutor` that validates capabilities and produces a
  deterministic SHA-256 telemetry digest, plus a runnable robot-side demo
  (`cargo run -p example-robot --bin simulated-robot`). A native `rclrs`
  example node is compiled in the ROS CI job.
- `ros-bridge` `rosbridge` transport: JSON-over-WebSocket rosbridge_suite
  protocol on the configured capabilities/tasks/proofs topics. It compiles on
  every platform and is tested against a local rosbridge-style WebSocket server
  (subscribe, task delivery, advertise, and proof publish). It has not been
  exercised against a real ROS 2 rosbridge deployment here.
- `stellar-bridge`: a real `soroban-client` 0.6 backed `Settlement`
  implementation. Request construction, contract-error mapping, and contract
  status parsing are unit-tested offline, and the full
  `create -> accept -> proof -> release` lifecycle has been run against the
  testnet TaskEscrow (see [docs/architecture.md](docs/architecture.md#verified-testnet-deployment)).

CI-verified only:

- `ros-bridge` `rclrs` transport and the `native_robot` example, behind
  `--features ros`. They compile and test under ROS 2 Jazzy in CI; no ROS
  runtime is available in this development environment, so the live graph was
  not exercised here.

Still to come:

- Live wiring of a transport and the settlement client into `bridge-node`
  beyond the in-memory demo; a local Soroban sandbox test.
- Dispute handling and the M4 hardening work.

See the [roadmap](docs/roadmap.md) and the open
[Stellar Wave issues](https://github.com/Primex-tech-labs/stellar-ros-bridge/issues?q=is%3Aissue+label%3A%22Stellar+Wave%22).

## Getting started

Prerequisites:

- Rust 1.85+ (see `rust-toolchain.toml`; installed automatically by rustup)
- ROS 2 Jazzy or newer only when building the `rclrs` transport
- A funded Stellar testnet account for manual smoke tests

```bash
git clone https://github.com/Primex-tech-labs/stellar-ros-bridge
cd stellar-ros-bridge
cargo build --workspace
cargo test --workspace

# Run the lifecycle once with an in-memory transport and settlement:
cargo run -p bridge-node -- --demo

# Run the simulated robot-side lifecycle (validate, execute, prove, settle):
cargo run -p example-robot --bin simulated-robot

# Contract tests (no Stellar network required):
cargo test --manifest-path contracts/task-escrow/Cargo.toml
```

### ROS 2 transports

The portable transport speaks the rosbridge_suite protocol (a WebSocket server
in front of a ROS 2 graph):

```bash
# Default build includes the rosbridge transport; no ROS installation needed.
cargo build -p ros-bridge

# Native graph transport; requires ROS 2 Jazzy and a sourced environment.
source /opt/ros/jazzy/setup.bash
cargo build -p ros-bridge --features ros
```

Both transports use `std_msgs/msg/String` JSON payloads:

- capabilities topic: serialized `Capability`
- tasks topic: serialized `Task` (inbound)
- proofs topic: serialized `Proof` (outbound)

The bridge does not guess how to drive a robot. Supply a `TaskExecutor`
implementation to `RosbridgeTransport::with_executor` or
`RclrsTransport::with_executor`. Without one, `execute` fails with a clear
error instead of publishing a fabricated proof.

### Live settlement

`StellarSettlement` implements the `bridge_core::Settlement` trait against the
deployed TaskEscrow contract. Configure it from environment variables (see
[`.env.example`](.env.example)):

- `STELLAR_RPC_URL`
- `STELLAR_NETWORK` (`testnet`, `mainnet`, `futurenet`, `standalone`, or a raw passphrase)
- `STELLAR_SECRET` (never commit or log)
- `TASK_ESCROW_CONTRACT_ID`
- `STELLAR_TOKEN_CONTRACT_ID` (required for native XLM rewards)
- optional: `STELLAR_FEE`, `STELLAR_TIMEOUT_SECONDS`,
  `STELLAR_RPC_TIMEOUT_SECONDS`, `STELLAR_ALLOW_HTTP`

One `StellarSettlement` instance is one Stellar account. Use separate instances
for requester and executor: the contract authorizes `create_task`, `release`,
and `refund` as the requester, and `accept_task` / `submit_proof` as the
executor.

Manual testnet smoke test (real network calls, never run by `cargo test`):

```bash
# Create a task, read its state, and optionally run the full lifecycle with a
# distinct executor account.
export SMOKE_EXECUTOR_SECRET=S...   # optional, must differ from STELLAR_SECRET
export STELLAR_TOKEN_CONTRACT_ID=CDLZ...  # XLM SAC for the target network
cargo run -p stellar-bridge --bin stellar-testnet-smoke
```

## Contributing

We run contributions through the **Drips Wave** program for the Stellar
ecosystem. Please read:

- [`CONTRIBUTING.md`](CONTRIBUTING.md) - workflow, commit convention, review bar
- [`docs/issue-guide.md`](docs/issue-guide.md) - how our issues are written
- [`.wave/issues.md`](.wave/issues.md) - the current Wave backlog

Issues labeled **`Stellar Wave`** are in scope for a Wave. Ask to be assigned
before starting; unassigned PRs may be closed to keep the sprint fair.

## ROS bridge parameters

The ROS bridge configuration module supports ROS 2 YAML parameter files. The
example defaults are in `config/ros-bridge-params.yaml`:

- `capabilities_topic`: `/robot/capabilities`
- `tasks_topic`: `/robot/tasks`
- `proofs_topic`: `/robot/proofs`
- `ros_domain_id`: `0` (valid range: 0-232)

Use a copy of the example file to customize these values for your deployment.

## License

Apache-2.0. See [`LICENSE`](LICENSE).
