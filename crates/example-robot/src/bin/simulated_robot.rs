//! Runnable robot-side demo: advertise, accept, execute, prove, settle.
//!
//! The transport is in-memory so the demo needs no ROS installation and no
//! network. It exercises the same `BridgeNode`/`SimulatedRobot` code paths a
//! real transport uses.

use std::collections::VecDeque;
use std::sync::Mutex;

use bridge_core::driver::BridgeNode;
use bridge_core::mock::{MockClock, MockSettlement};
use bridge_core::{
    AccountId, AssetId, Capability, CapabilityId, Proof, Reward, Settlement, Task, TaskId,
    Transport, TransportError,
};
use example_robot::SimulatedRobot;
use serde_json::json;

struct RobotTransport {
    robot: SimulatedRobot,
    tasks: Mutex<VecDeque<Task>>,
    advertised: Mutex<Vec<Capability>>,
    published: Mutex<Vec<Proof>>,
}

impl RobotTransport {
    fn with_tasks(robot: SimulatedRobot, tasks: Vec<Task>) -> Self {
        Self {
            robot,
            tasks: Mutex::new(tasks.into()),
            advertised: Mutex::new(Vec::new()),
            published: Mutex::new(Vec::new()),
        }
    }

    fn published(&self) -> Vec<Proof> {
        self.published.lock().expect("published lock").clone()
    }
}

impl Transport for RobotTransport {
    fn advertise(&self, capability: &Capability) -> Result<(), TransportError> {
        self.advertised
            .lock()
            .expect("advertised lock")
            .push(capability.clone());
        Ok(())
    }

    fn next_task(&self) -> Result<Option<Task>, TransportError> {
        Ok(self.tasks.lock().expect("tasks lock").pop_front())
    }

    fn execute(&self, task: &Task) -> Result<Proof, TransportError> {
        ros_bridge::TaskExecutor::execute(&self.robot, task)
    }

    fn publish_proof(&self, proof: &Proof) -> Result<(), TransportError> {
        self.published
            .lock()
            .expect("published lock")
            .push(proof.clone());
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let requester = AccountId::new(format!("G{}", "A".repeat(55)))?;
    let robot_id = AccountId::new(format!("G{}", "B".repeat(55)))?;
    let capability = CapabilityId::new("navigate")?;

    let settlement = MockSettlement::new();
    let placeholder = Task {
        id: TaskId(0),
        capability: capability.clone(),
        params: json!({"x": 1.0, "y": 2.0}),
        reward: Reward::new(AssetId::Native, 1)?,
        deadline: 2_000_000,
        requester: requester.clone(),
    };
    let id = settlement.create_task(&placeholder)?;
    let task = Task { id, ..placeholder };

    let robot = SimulatedRobot::with_capabilities(robot_id.clone(), [capability.clone()]);
    let transport = RobotTransport::with_tasks(robot, vec![task]);
    transport.advertise(&Capability {
        id: capability,
        version: 1,
        units: "per_task".to_owned(),
        metadata: json!({"description": "simulated navigation"}),
    })?;

    let node = BridgeNode::new(transport, settlement, MockClock::at(1), robot_id);
    let outcome = node.run_once()?.expect("one queued task");

    println!("robot task {} -> {:?}", outcome.task_id, outcome.state);
    for proof in node.transport().published() {
        println!(
            "proof task={} digest={} signer={}",
            proof.task_id,
            proof
                .digest
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
            proof.signer
        );
    }
    Ok(())
}
