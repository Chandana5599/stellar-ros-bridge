//! Core domain types, traits and lifecycle state machine for Stellar ROS Bridge.
//!
//! This crate is intentionally free of ROS 2 and Stellar dependencies so the
//! task lifecycle can be unit-tested with mocks. See `docs/architecture.md`.

#![forbid(unsafe_code)]

pub mod driver;
pub mod error;
pub mod mock;
pub mod state;
pub mod traits;
pub mod types;

pub use driver::{BridgeNode, NodeError, Outcome};
pub use error::{CoreError, SettlementError, TransportError};
pub use state::{TaskEvent, TaskState};
pub use traits::{Clock, Settlement, Transport};
pub use types::{AccountId, AssetId, Capability, CapabilityId, Proof, Reward, Task, TaskId};

/// The crate version, sourced from `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
