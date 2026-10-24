# Changelog

All notable changes to Stellar ROS Bridge are documented here. This project
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and
[Semantic Versioning](https://semver.org/).

## [0.1.0] - 2026-10-21

### Added

- `bridge-core`: validated `AccountId`/`CapabilityId`/`Reward` types, `Task`,
  `Proof`, the task state machine, `Transport`/`Settlement`/`Clock` traits,
  mocks and the `BridgeNode` driver.
- `contracts/task-escrow`: Soroban escrow with authorization, deadlines, proof
  digests, TTL bumps, release/refund and invariant tests.
- `ros-bridge`: portable rosbridge_suite JSON-over-WebSocket transport and a
  native `rclrs` transport behind the `ros` feature, both using
  `std_msgs/msg/String` JSON.
- `stellar-bridge`: live `soroban-client` settlement (build, simulate, sign,
  submit, poll) with contract-error and state parsing.
- `example-robot`: capability validation, SHA-256 telemetry digest and a
  runnable simulated robot lifecycle; a native `rclrs` example.
- Live testnet proof: `create -> accept -> proof -> release` on the deployed
  TaskEscrow `CD55XYVX5AXAJNKBKALP6AESUNA2NZHTWOCNK3QUW4EYHGTN4LIHS46X`.
- ROS 2 Jazzy CI job, message-schema docs and a TaskEscrow deploy script.

### Fixed

- Corrected the rclrs dynamic-message imports to the public 0.8 API.
- Increased the default RPC timeout for public endpoint cold starts.

### Notes

- A robot-specific executor is included; a live ROS graph run still requires a
  ROS 2 installation in the target environment.
