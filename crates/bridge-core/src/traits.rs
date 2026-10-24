//! Boundaries between the domain and the outside world.
//!
//! Keeping these traits free of ROS 2 and Stellar types lets the lifecycle be
//! tested with mocks and lets adapters (rclrs, Soroban RPC) evolve separately.

use crate::error::{SettlementError, TransportError};
use crate::state::TaskState;
use crate::types::{AccountId, Capability, Proof, Task, TaskId};

/// Publishes capabilities and exchanges tasks with the ROS 2 graph.
pub trait Transport: Send + Sync {
    fn advertise(&self, capability: &Capability) -> Result<(), TransportError>;
    /// Return the next task to execute, if one is waiting.
    fn next_task(&self) -> Result<Option<Task>, TransportError>;
    /// Execute the task on the robot and return proof of completion.
    fn execute(&self, task: &Task) -> Result<Proof, TransportError>;
    fn publish_proof(&self, proof: &Proof) -> Result<(), TransportError>;
}

/// Creates and settles escrows on Stellar.
pub trait Settlement: Send + Sync {
    fn create_task(&self, task: &Task) -> Result<TaskId, SettlementError>;
    fn accept_task(&self, id: TaskId, executor: &AccountId) -> Result<(), SettlementError>;
    fn submit_proof(&self, id: TaskId, proof: &Proof) -> Result<(), SettlementError>;
    fn release(&self, id: TaskId) -> Result<(), SettlementError>;
    fn refund(&self, id: TaskId) -> Result<(), SettlementError>;
    fn get_state(&self, id: TaskId) -> Result<TaskState, SettlementError>;
}

/// Source of ledger time; injected so tests control expiry.
pub trait Clock: Send + Sync {
    fn now(&self) -> u64;
}
