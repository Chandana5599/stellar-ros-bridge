//! Task lifecycle state machine.
//!
//! ```text
//! Announced -> Requested -> Accepted -> InProgress -> ProofSubmitted -> Settled
//!                                \__________ Dispute __________/
//! Requested/Accepted/InProgress --Expire--> Expired
//! ```

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskEvent {
    Request,
    Accept,
    Start,
    SubmitProof,
    Settle,
    Dispute,
    Expire,
}

impl TaskState {
    /// Apply an event, rejecting transitions that would skip or repeat a step.
    pub fn transition(self, event: TaskEvent) -> Result<Self, CoreError> {
        use TaskEvent::*;
        use TaskState::*;

        let next = match (self, event) {
            (Announced, Request) => Requested,
            (Requested, Accept) => Accepted,
            (Accepted, Start) => InProgress,
            (InProgress, SubmitProof) => ProofSubmitted,
            (ProofSubmitted, Settle) => Settled,
            (Accepted | InProgress | ProofSubmitted, Dispute) => Disputed,
            (Requested | Accepted | InProgress, Expire) => Expired,
            _ => return Err(CoreError::InvalidTransition { from: self, event }),
        };
        Ok(next)
    }

    /// Terminal states cannot change again.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            TaskState::Settled | TaskState::Disputed | TaskState::Expired
        )
    }
}
