//! Error model for the domain and the transport/settlement boundaries.

use crate::state::{TaskEvent, TaskState};

/// Validation and lifecycle errors.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CoreError {
    #[error("invalid Stellar account id: {0}")]
    InvalidAccountId(String),
    #[error("invalid capability id: {0}")]
    InvalidCapabilityId(String),
    #[error("reward must be positive, got {0}")]
    InvalidReward(i128),
    #[error("invalid transition from {from:?} on {event:?}")]
    InvalidTransition { from: TaskState, event: TaskEvent },
    #[error("proof belongs to task {actual}, expected {expected}")]
    ProofTaskMismatch { expected: u64, actual: u64 },
}

/// Failure while talking to the ROS graph.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("transport error: {message}")]
pub struct TransportError {
    pub message: String,
}

impl TransportError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// Failure while talking to the Stellar network or the escrow contract.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("settlement error: {message}")]
pub struct SettlementError {
    pub message: String,
}

impl SettlementError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}
