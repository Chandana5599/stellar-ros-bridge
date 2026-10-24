//! A robot-specific [`TaskExecutor`](ros_bridge::TaskExecutor) implementation.
//!
//! This crate is deliberately small: it shows how an application turns a task
//! into a proof. The simulated robot validates the advertised capability and
//! hashes the task's capability/params into a 32-byte telemetry digest. A real
//! robot would replace `TaskExecutor::execute` with its own SDK call and
//! sign the telemetry with the robot key.

#![forbid(unsafe_code)]

use bridge_core::{AccountId, CapabilityId, Proof, Task, TransportError};
use sha2::{Digest, Sha256};

/// Robot-side executor that deterministically "completes" tasks.
#[derive(Clone, Debug)]
pub struct SimulatedRobot {
    identity: AccountId,
    capabilities: Option<Vec<CapabilityId>>,
}

impl SimulatedRobot {
    /// Build a robot that accepts any capability.
    pub fn new(identity: AccountId) -> Self {
        Self {
            identity,
            capabilities: None,
        }
    }

    /// Restrict the robot to an advertised set of capabilities.
    pub fn with_capabilities(
        identity: AccountId,
        capabilities: impl IntoIterator<Item = CapabilityId>,
    ) -> Self {
        Self {
            identity,
            capabilities: Some(capabilities.into_iter().collect()),
        }
    }

    pub fn identity(&self) -> &AccountId {
        &self.identity
    }

    fn accepts(&self, capability: &CapabilityId) -> bool {
        match &self.capabilities {
            None => true,
            Some(capabilities) => capabilities.contains(capability),
        }
    }

    /// Telemetry digest for a task. Exposed so tests and operators can verify
    /// what the robot signed.
    pub fn digest_for(task: &Task) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(task.id.0.to_le_bytes());
        hasher.update(b":");
        hasher.update(task.capability.as_str().as_bytes());
        hasher.update(b":");
        hasher.update(task.params.to_string().as_bytes());
        hasher.finalize().into()
    }
}

impl ros_bridge::TaskExecutor for SimulatedRobot {
    fn execute(&self, task: &Task) -> Result<Proof, TransportError> {
        if !self.accepts(&task.capability) {
            return Err(TransportError::new(format!(
                "robot {} does not advertise capability {}",
                self.identity, task.capability
            )));
        }
        Ok(Proof {
            task_id: task.id,
            digest: Self::digest_for(task),
            signer: self.identity.clone(),
            // A real robot signs `digest` with its Stellar key. The simulated
            // robot leaves the signature empty so it cannot be mistaken for a
            // production attestation.
            signature: [0u8; 64],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bridge_core::{AssetId, Reward};
    use ros_bridge::TaskExecutor;
    use serde_json::json;

    fn account(prefix: char) -> AccountId {
        AccountId::new(format!("{prefix}{}", "A".repeat(55))).unwrap()
    }

    fn task() -> Task {
        Task {
            id: bridge_core::TaskId(7),
            capability: CapabilityId::new("navigate").unwrap(),
            params: json!({"x": 1.5, "y": 2.5}),
            reward: Reward::new(AssetId::Native, 10).unwrap(),
            deadline: 1_700_000_000,
            requester: account('G'),
        }
    }

    #[test]
    fn executes_and_signs_the_task_id() {
        let robot = SimulatedRobot::new(account('C'));
        let task = task();
        let proof = robot.execute(&task).unwrap();
        assert_eq!(proof.task_id, task.id);
        assert_eq!(proof.signer, robot.identity);
        assert_eq!(proof.digest, SimulatedRobot::digest_for(&task));
    }

    #[test]
    fn digest_changes_with_params() {
        let mut changed = task();
        changed.params = json!({"x": 9.0, "y": 9.0});
        assert_ne!(
            SimulatedRobot::digest_for(&task()),
            SimulatedRobot::digest_for(&changed)
        );
    }

    #[test]
    fn rejects_unadvertised_capabilities() {
        let robot =
            SimulatedRobot::with_capabilities(account('C'), [CapabilityId::new("pick").unwrap()]);
        let error = robot.execute(&task()).unwrap_err();
        assert!(error.to_string().contains("does not advertise"));
    }
}
