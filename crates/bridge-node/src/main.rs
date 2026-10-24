//! Command-line entry point for Stellar ROS Bridge.

use bridge_core::driver::BridgeNode;
use bridge_core::mock::{MockClock, MockSettlement, MockTransport};
use bridge_core::{
    AccountId, AssetId, Capability, CapabilityId, Reward, Settlement, Task, TaskId, Transport,
};
use serde_json::json;
use std::env;
use std::error::Error;
use std::io;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let mut params_file: Option<PathBuf> = None;
    let mut demo = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--params-file" => {
                let path = args.next().ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "--params-file requires a file path",
                    )
                })?;
                params_file = Some(PathBuf::from(path));
            }
            "--demo" => demo = true,
            "--help" | "-h" => {
                println!(
                    "Usage: bridge-node [--demo] [--params-file PATH]\n\
                     --demo runs the task lifecycle with an in-memory transport/settlement.\n\
                     Alternatively, set ROS_BRIDGE_PARAMS_FILE to a parameter file."
                );
                return Ok(());
            }
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("Unknown argument: {arg}"),
                )
                .into());
            }
        }
    }

    let params_file =
        params_file.or_else(|| env::var_os("ROS_BRIDGE_PARAMS_FILE").map(PathBuf::from));

    let config = bridge_node::load_config(params_file.as_deref())?;

    // Set this before any future ROS context initialization.
    env::set_var("ROS_DOMAIN_ID", config.ros_domain_id.to_string());

    if demo {
        run_demo()?;
        return Ok(());
    }

    tracing::info!(
        version = bridge_node::VERSION,
        capabilities_topic = %config.capabilities_topic,
        tasks_topic = %config.tasks_topic,
        proofs_topic = %config.proofs_topic,
        ros_domain_id = config.ros_domain_id,
        "Stellar ROS Bridge configuration loaded (live transports are not wired into this binary)"
    );

    println!("stellar-ros-bridge {}", bridge_node::VERSION);
    println!("capabilities_topic={}", config.capabilities_topic);
    println!("tasks_topic={}", config.tasks_topic);
    println!("proofs_topic={}", config.proofs_topic);
    println!("ros_domain_id={}", config.ros_domain_id);
    println!("ROS_DOMAIN_ID={}", env::var("ROS_DOMAIN_ID")?);

    let address = env::var("HEALTH_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_owned());
    let listener = match tokio::net::TcpListener::bind(&address).await {
        Ok(listener) => listener,
        Err(error) => {
            tracing::error!(%address, %error, "Failed to bind health server");
            return Err(Box::<dyn Error>::from(error));
        }
    };

    tracing::info!(
        version = bridge_node::VERSION,
        %address,
        "Stellar ROS Bridge health server started"
    );

    // Dependency checks are not wired into the current scaffolding yet.
    let state = bridge_node::health::HealthState::new(true);
    axum::serve(listener, bridge_node::health::router(state)).await?;

    Ok(())
}

/// Run one complete task lifecycle with in-memory collaborators.
///
/// This is the executable smoke test for the domain: it advertises a
/// capability, accepts a funded task, executes it, publishes proof and settles
/// the escrow. It needs neither ROS 2 nor a Stellar network.
fn run_demo() -> Result<(), Box<dyn Error>> {
    let requester = AccountId::new(format!("G{}", "A".repeat(55)))?;
    let executor = AccountId::new(format!("G{}", "B".repeat(55)))?;
    let capability = Capability {
        id: CapabilityId::new("navigate")?,
        version: 1,
        units: "per_task".to_owned(),
        metadata: json!({"description": "demo navigation capability"}),
    };

    let settlement = MockSettlement::new();
    let base = Task {
        id: TaskId(0),
        capability: capability.id.clone(),
        params: json!({"x": 1.0, "y": 2.0}),
        reward: Reward::new(AssetId::Native, 100)?,
        deadline: 2_000_000,
        requester,
    };
    let id = settlement.create_task(&base)?;
    let task = Task { id, ..base };

    let transport = MockTransport::with_tasks(vec![task]);
    transport.advertise(&capability)?;

    let node = BridgeNode::new(transport, settlement, MockClock::at(1_000_000), executor);
    let outcomes = node.run_until_idle(1)?;
    tracing::info!(
        advertised = ?node.transport().advertised(),
        "demo capabilities advertised"
    );
    for outcome in outcomes {
        println!("demo task {} -> {:?}", outcome.task_id, outcome.state);
    }
    Ok(())
}
