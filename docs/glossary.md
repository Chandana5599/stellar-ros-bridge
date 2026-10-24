# Glossary

This glossary defines the main terms used in Stellar ROS Bridge. For technical details, see the linked documentation.

## Capability

A service a robot advertises, such as `navigate`, `pick`, or `inspect`. Capabilities are published through ROS 2 topics so tasks can be matched to the services a robot provides.

See [README](../README.md) and [Architecture](architecture.md).

## Task

A unit of paid work that matches a capability. A task includes parameters, a reward, and a deadline, and moves through the task lifecycle.

See [Architecture](architecture.md) and [Roadmap](roadmap.md).

## Proof

Evidence that a task has been completed. Examples include a signed telemetry digest, sensor attestation, or maintainer sign-off. The proof must satisfy the project's verification rules before escrowed funds can be released.

See [README](../README.md) and [Architecture](architecture.md).

## Escrow

A mechanism implemented by the planned Soroban `TaskEscrow` contract to hold a task's reward until the task is proven complete. Funds can be released to the executor or refunded according to the contract's rules.

See [Architecture](architecture.md) and [Contracts README](../contracts/README.md).

## Settlement

The process of releasing escrowed funds to the robot's Stellar account after the required proof is accepted. The Stellar bridge handles contract interaction and transaction confirmation.

See [Architecture](architecture.md) and [Roadmap](roadmap.md).

## Wave

A contribution cycle in the Drips Wave program. Work is organised into scoped issues that contributors can complete within the cycle.

See [Issue Guide](issue-guide.md), [Roadmap](roadmap.md), and the [Wave issue backlog](../.wave/issues.md).

## Points

The value assigned to a Wave issue based on its complexity. The project's issue guide defines the point values used to estimate and reward contributions:

| Complexity | Points |
| --- | ---: |
| Trivial | 100 |
| Medium | 150 |
| High | 200 |

See [Issue Guide](issue-guide.md) and the [Wave issue backlog](../.wave/issues.md).
