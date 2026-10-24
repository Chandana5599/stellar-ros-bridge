//! The runtime task driver: executes tasks from the transport and settles them.

use crate::{
    AccountId, Clock, CoreError, Proof, Settlement, SettlementError, Task, TaskEvent, TaskId,
    TaskState, Transport, TransportError,
};

/// Failure while driving one task through the lifecycle.
#[derive(Debug, thiserror::Error)]
pub enum NodeError {
    #[error("transport failed: {0}")]
    Transport(#[from] TransportError),
    #[error("settlement failed: {0}")]
    Settlement(#[from] SettlementError),
    #[error("invalid lifecycle transition: {0}")]
    Lifecycle(#[from] CoreError),
    #[error("proof is for task {actual}, expected {expected}")]
    WrongProof { expected: TaskId, actual: TaskId },
}

/// Result of handling a single task.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Outcome {
    pub task_id: TaskId,
    pub state: TaskState,
}

/// Wires a [`Transport`] to a [`Settlement`] with an injected [`Clock`].
pub struct BridgeNode<T, S, C> {
    transport: T,
    settlement: S,
    clock: C,
    identity: AccountId,
}

impl<T, S, C> BridgeNode<T, S, C>
where
    T: Transport,
    S: Settlement,
    C: Clock,
{
    pub fn new(transport: T, settlement: S, clock: C, identity: AccountId) -> Self {
        Self {
            transport,
            settlement,
            clock,
            identity,
        }
    }

    pub fn transport(&self) -> &T {
        &self.transport
    }

    pub fn settlement(&self) -> &S {
        &self.settlement
    }

    /// Accept, execute and settle a task that already exists on-chain.
    pub fn handle_task(&self, task: &Task) -> Result<Outcome, NodeError> {
        let mut state = TaskState::Announced;
        state = state.transition(TaskEvent::Request)?;
        state = state.transition(TaskEvent::Accept)?;
        self.settlement.accept_task(task.id, &self.identity)?;

        if self.clock.now() >= task.deadline {
            state = state.transition(TaskEvent::Expire)?;
            self.settlement.refund(task.id)?;
            return Ok(Outcome {
                task_id: task.id,
                state,
            });
        }

        let proof = self.transport.execute(task)?;
        Self::check_proof(task, &proof)?;

        state = state.transition(TaskEvent::Start)?;
        state = state.transition(TaskEvent::SubmitProof)?;
        self.settlement.submit_proof(task.id, &proof)?;
        self.transport.publish_proof(&proof)?;

        state = state.transition(TaskEvent::Settle)?;
        self.settlement.release(task.id)?;
        Ok(Outcome {
            task_id: task.id,
            state,
        })
    }

    /// Handle the next waiting task, or return `None` when the queue is empty.
    pub fn run_once(&self) -> Result<Option<Outcome>, NodeError> {
        match self.transport.next_task()? {
            Some(task) => self.handle_task(&task).map(Some),
            None => Ok(None),
        }
    }

    /// Handle up to `max_tasks` tasks. Stops early when the queue is empty.
    pub fn run_until_idle(&self, max_tasks: usize) -> Result<Vec<Outcome>, NodeError> {
        let mut outcomes = Vec::new();
        for _ in 0..max_tasks {
            match self.run_once()? {
                Some(outcome) => outcomes.push(outcome),
                None => break,
            }
        }
        Ok(outcomes)
    }

    fn check_proof(task: &Task, proof: &Proof) -> Result<(), NodeError> {
        if proof.task_id != task.id {
            return Err(NodeError::WrongProof {
                expected: task.id,
                actual: proof.task_id,
            });
        }
        Ok(())
    }
}
