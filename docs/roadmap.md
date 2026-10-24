# Roadmap

This is a phased plan. Each milestone lists the outcome, not a date. Work is
sliced into Wave-sized issues in `.wave/issues.md`.

## M0 - Foundation (done)

**Outcome:** a compiling workspace with a stable domain model and CI.

- [x] `bridge-core` domain types + error model with round-trip tests
- [x] Repository scaffolding, CI (fmt/clippy/test), label + issue process
- [x] Documented architecture and local dev setup

## M1 - Task lifecycle off-chain (done)

**Outcome:** the whole task lifecycle works with mocks - no chain, no robot.

- [x] `Transport` and `Settlement` traits
- [x] `bridge-node` state machine (`Announced -> ... -> Settled`) with unit tests
- [x] Mock transport + mock settlement used in integration tests

## M2 - Stellar settlement

**Outcome:** a task can be escrowed and released on **testnet**.

- [x] `TaskEscrow` Soroban contract with the invariants from `architecture.md`
- [x] `stellar-bridge` client: create/accept/proof/release/refund/state
- [x] Pure request-construction, status-parsing, and contract-error mapping tests
- [x] Manual testnet smoke binary (`stellar-testnet-smoke`)
- [x] End-to-end test against a live testnet deployment (manual smoke)
- [ ] End-to-end test against a local Soroban sandbox

The client is implemented with `soroban-client` 0.6. Its offline tests pass, and
the full create/accept/proof/release flow was run on testnet (contract
`CD55XYVX5AXAJNKBKALP6AESUNA2NZHTWOCNK3QUW4EYHGTN4LIHS46X`; see
`docs/architecture.md`). Live calls remain manual and are not part of
`cargo test`.

## M3 - ROS 2 integration

**Outcome:** a real ROS 2 node drives the lifecycle.

- [x] `rosbridge` JSON-over-WebSocket transport (portable, default feature)
- [x] `rclrs` transport behind the `ros` feature (dynamic String messages)
- [x] CI job that builds/tests `ros-bridge --features ros` under ROS 2 Jazzy
- [x] Example robot package that advertises a capability and completes a task
- [x] Robot-specific `TaskExecutor` implementations

The `rosbridge` transport is tested against a local mock WebSocket server but
not exercised against a real ROS 2 rosbridge deployment here. The `rclrs`
transport and `native_robot` example compile/test under Jazzy in CI; no ROS
runtime is available locally.

## M4 - Hardening

**Outcome:** safe enough for a pilot fleet.

- [ ] Dispute resolution path and maintainer review flow
- [ ] Fuzz/negative tests for proof verification and escrow math
- [ ] Observability: tracing spans, metrics, and a health endpoint
- [ ] Threat model and security review checklist
- [ ] Live wiring of transport + settlement into `bridge-node` (beyond demo)

The basic health endpoint already exists; broader metrics and tracing spans
remain.

## Non-goals (for now)

- Multi-chain settlement.
- A hosted service or dashboard.
- Real-money (mainnet) operation before M4 is complete.
