//! Example native ROS 2 robot node.
//!
//! Build and run with a sourced ROS 2 environment:
//!
//! ```bash
//! cargo run -p ros-bridge --features ros --example native_robot
//! ros2 topic pub /robot/tasks std_msgs/msg/String \
//!   "{data: '{\"id\":1,\"capability\":\"navigate\",...}'}"
//! ```
//!
//! The example uses a tiny inline executor. A real robot should replace
//! `EchoRobot` with its own action/service call and sign the telemetry digest.

#[cfg(feature = "ros")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::time::Duration;

    use bridge_core::{
        AccountId, Capability, CapabilityId, Proof, Task, Transport, TransportError,
    };
    use ros_bridge::config::NodeConfig;
    use ros_bridge::rclrs_transport::RclrsTransport;
    use serde_json::json;

    struct EchoRobot {
        identity: AccountId,
    }

    impl ros_bridge::TaskExecutor for EchoRobot {
        fn execute(&self, task: &Task) -> Result<Proof, TransportError> {
            let mut digest = [0u8; 32];
            digest[..8].copy_from_slice(&task.id.0.to_le_bytes());
            Ok(Proof {
                task_id: task.id,
                digest,
                signer: self.identity.clone(),
                // Replace with a real signature in production.
                signature: [0u8; 64],
            })
        }
    }

    let identity = AccountId::new(format!("G{}", "A".repeat(55)))?;
    let transport = RclrsTransport::with_executor(NodeConfig::default(), EchoRobot { identity })?;
    transport.advertise(&Capability {
        id: CapabilityId::new("navigate")?,
        version: 1,
        units: "per_task".to_owned(),
        metadata: json!({"example": true}),
    })?;

    println!("native robot advertised 'navigate'; waiting for tasks...");
    loop {
        if let Some(task) = transport.next_task()? {
            let proof = transport.execute(&task)?;
            transport.publish_proof(&proof)?;
            println!("executed task {}", task.id);
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

#[cfg(not(feature = "ros"))]
fn main() {
    eprintln!("build with --features ros and a sourced ROS 2 Jazzy environment");
}
