# Soroban contracts

On-chain contracts for Stellar ROS Bridge, written in Rust against
[`soroban-sdk`](https://docs.rs/soroban-sdk).

```
contracts/
  task-escrow/        # TaskEscrow: create/accept/prove/release/refund
    Cargo.toml
    src/lib.rs
    src/test.rs
```

## TaskEscrow

The contract holds a task reward in escrow and releases it to the robot only
after the requester accepts a proof:

- `create_task(requester, capability, token, amount, deadline)` transfers the
  reward into the contract and assigns a task id. The requester authorizes it.
- `accept_task(task_id, executor)` lets a robot claim an open, unexpired task.
- `submit_proof(task_id, executor, digest)` records a 32-byte proof digest
  before the deadline; only the accepted executor may call it.
- `release(task_id)` pays the executor once a proof exists. Requester-authorized.
- `refund(task_id)` returns the reward after the deadline if the task was not
  released. Requester-authorized.
- `get_task(task_id)` returns the stored state.

Safety invariants (covered by `src/test.rs`):

1. A task can only be released once, and only after a proof.
2. Release and refund are mutually exclusive.
3. Only the accepted executor can submit proof.
4. Proofs cannot be submitted, and tasks cannot be accepted, after the deadline.
5. The requester cannot accept the task they created.
6. Persistent entries are TTL-bumped on every read/write.

## Build and test

```bash
cd contracts/task-escrow
cargo test                       # unit tests with soroban-sdk testutils
cargo build --release --target wasm32v1-none
# or, with stellar-cli installed:
stellar contract build
```

The contract is its own Cargo workspace so Soroban dependencies do not leak into
the bridge crates.
