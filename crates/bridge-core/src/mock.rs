//! In-memory implementations used by the node tests and the demo mode.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

use crate::error::{SettlementError, TransportError};
use crate::state::TaskState;
use crate::traits::{Clock, Settlement, Transport};
use crate::types::{AccountId, Capability, Proof, Task, TaskId};

/// Deterministic ledger clock.
#[derive(Clone, Debug, Default)]
pub struct MockClock {
    pub now: u64,
}

impl MockClock {
    pub fn at(now: u64) -> Self {
        Self { now }
    }
}

impl Clock for MockClock {
    fn now(&self) -> u64 {
        self.now
    }
}

/// Transport that hands out a fixed queue of tasks and records every call.
#[derive(Default)]
pub struct MockTransport {
    capabilities: Mutex<Vec<Capability>>,
    tasks: Mutex<VecDeque<Task>>,
    executed: Mutex<Vec<TaskId>>,
    published: Mutex<Vec<Proof>>,
}

impl MockTransport {
    pub fn with_tasks(tasks: Vec<Task>) -> Self {
        Self {
            tasks: Mutex::new(tasks.into()),
            ..Self::default()
        }
    }

    pub fn advertised(&self) -> Vec<Capability> {
        self.capabilities.lock().expect("capabilities lock").clone()
    }

    pub fn executed(&self) -> Vec<TaskId> {
        self.executed.lock().expect("executed lock").clone()
    }

    pub fn published(&self) -> Vec<Proof> {
        self.published.lock().expect("published lock").clone()
    }
}

impl Transport for MockTransport {
    fn advertise(&self, capability: &Capability) -> Result<(), TransportError> {
        self.capabilities
            .lock()
            .expect("capabilities lock")
            .push(capability.clone());
        Ok(())
    }

    fn next_task(&self) -> Result<Option<Task>, TransportError> {
        Ok(self.tasks.lock().expect("tasks lock").pop_front())
    }

    fn execute(&self, task: &Task) -> Result<Proof, TransportError> {
        self.executed.lock().expect("executed lock").push(task.id);
        Ok(Proof {
            task_id: task.id,
            digest: [7u8; 32],
            signer: task.requester.clone(),
            signature: [0u8; 64],
        })
    }

    fn publish_proof(&self, proof: &Proof) -> Result<(), TransportError> {
        self.published
            .lock()
            .expect("published lock")
            .push(proof.clone());
        Ok(())
    }
}

/// Settlement that records the lifecycle calls without touching a network.
#[derive(Default)]
pub struct MockSettlement {
    next_id: Mutex<u64>,
    states: Mutex<HashMap<TaskId, TaskState>>,
    created: Mutex<Vec<Task>>,
    accepted: Mutex<Vec<(TaskId, AccountId)>>,
    submitted: Mutex<Vec<(TaskId, Proof)>>,
    released: Mutex<Vec<TaskId>>,
    refunded: Mutex<Vec<TaskId>>,
}

impl MockSettlement {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn created(&self) -> Vec<Task> {
        self.created.lock().expect("created lock").clone()
    }

    pub fn accepted(&self) -> Vec<(TaskId, AccountId)> {
        self.accepted.lock().expect("accepted lock").clone()
    }

    pub fn submitted(&self) -> Vec<(TaskId, Proof)> {
        self.submitted.lock().expect("submitted lock").clone()
    }

    pub fn released(&self) -> Vec<TaskId> {
        self.released.lock().expect("released lock").clone()
    }

    pub fn refunded(&self) -> Vec<TaskId> {
        self.refunded.lock().expect("refunded lock").clone()
    }

    fn state_of(&self, id: TaskId) -> Result<TaskState, SettlementError> {
        self.states
            .lock()
            .expect("states lock")
            .get(&id)
            .copied()
            .ok_or_else(|| SettlementError::new(format!("task {id:?} not found")))
    }

    fn set_state(&self, id: TaskId, state: TaskState) {
        self.states.lock().expect("states lock").insert(id, state);
    }
}

impl Settlement for MockSettlement {
    fn create_task(&self, task: &Task) -> Result<TaskId, SettlementError> {
        let mut next = self.next_id.lock().expect("next id lock");
        *next += 1;
        let id = TaskId(*next);
        self.set_state(id, TaskState::Requested);
        self.created
            .lock()
            .expect("created lock")
            .push(task.clone());
        Ok(id)
    }

    fn accept_task(&self, id: TaskId, executor: &AccountId) -> Result<(), SettlementError> {
        if self.state_of(id)? != TaskState::Requested {
            return Err(SettlementError::new(format!(
                "task {id:?} is not requestable"
            )));
        }
        self.set_state(id, TaskState::Accepted);
        self.accepted
            .lock()
            .expect("accepted lock")
            .push((id, executor.clone()));
        Ok(())
    }

    fn submit_proof(&self, id: TaskId, proof: &Proof) -> Result<(), SettlementError> {
        if self.state_of(id)? != TaskState::Accepted {
            return Err(SettlementError::new(format!("task {id:?} is not accepted")));
        }
        self.set_state(id, TaskState::ProofSubmitted);
        self.submitted
            .lock()
            .expect("submitted lock")
            .push((id, proof.clone()));
        Ok(())
    }

    fn release(&self, id: TaskId) -> Result<(), SettlementError> {
        if self.state_of(id)? != TaskState::ProofSubmitted {
            return Err(SettlementError::new(format!(
                "task {id:?} has no submitted proof"
            )));
        }
        self.set_state(id, TaskState::Settled);
        self.released.lock().expect("released lock").push(id);
        Ok(())
    }

    fn refund(&self, id: TaskId) -> Result<(), SettlementError> {
        if self.state_of(id)? != TaskState::Accepted {
            return Err(SettlementError::new(format!(
                "task {id:?} cannot be refunded"
            )));
        }
        self.set_state(id, TaskState::Expired);
        self.refunded.lock().expect("refunded lock").push(id);
        Ok(())
    }

    fn get_state(&self, id: TaskId) -> Result<TaskState, SettlementError> {
        self.state_of(id)
    }
}
