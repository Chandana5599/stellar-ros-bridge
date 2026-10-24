//! Native ROS 2 transport built on `rclrs` and dynamic messages.
//!
//! The implementation uses `std_msgs/msg/String` with a JSON `data` field, so it
//! needs no generated message bindings at compile time. The message type support
//! libraries are loaded from the sourced ROS 2 installation at runtime.
//!
//! This module is only compiled with `--features ros`. It has not been built in
//! an environment without ROS 2; the CI `ros` job builds it under Jazzy.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use bridge_core::{Capability, Proof, Task, Transport, TransportError};
use rclrs::rosidl_runtime_rs::String as RosString;
use rclrs::{
    Context, CreateBasicExecutor, DynamicMessage, DynamicMessageMetadata, DynamicPublisher,
    DynamicSubscription, ExecutorCommands, InitOptions, MessageTypeName, Node, SimpleValue,
    SimpleValueMut, SpinOptions, Value, ValueMut,
};

use crate::config::NodeConfig;
use crate::{TaskExecutor, UnsupportedExecutor};

const NODE_NAME: &str = "stellar_ros_bridge";

/// A [`Transport`] that publishes and subscribes on the native ROS 2 graph.
pub struct RclrsTransport<E = UnsupportedExecutor> {
    // Drop order matters: entities and the node must go before the context.
    capabilities_publisher: DynamicPublisher,
    proofs_publisher: DynamicPublisher,
    _tasks_subscription: DynamicSubscription,
    metadata: DynamicMessageMetadata,
    _node: Node,
    _context: Context,
    executor_commands: Arc<ExecutorCommands>,
    spin_thread: Option<JoinHandle<()>>,
    queued_tasks: Arc<Mutex<VecDeque<Task>>>,
    executor: E,
}

impl RclrsTransport<UnsupportedExecutor> {
    /// Create the node with the default executor, which rejects every task.
    pub fn new(config: NodeConfig) -> Result<Self, TransportError> {
        Self::with_executor(config, UnsupportedExecutor)
    }
}

impl<E> RclrsTransport<E>
where
    E: TaskExecutor,
{
    /// Create the ROS node, publishers, and tasks subscription, and spin the
    /// executor on a background thread.
    pub fn with_executor(config: NodeConfig, executor: E) -> Result<Self, TransportError> {
        let init_options = InitOptions::new().with_domain_id(Some(config.ros_domain_id as usize));
        let context = Context::new(std::env::args(), init_options).map_err(rclrs_error)?;
        let mut ros_executor = context.create_basic_executor();
        let node = ros_executor.create_node(NODE_NAME).map_err(rclrs_error)?;

        let message_type = message_type();
        let metadata = DynamicMessageMetadata::new(message_type.clone()).map_err(rclrs_error)?;

        let capabilities_publisher = node
            .create_dynamic_publisher(message_type.clone(), config.capabilities_topic.as_str())
            .map_err(rclrs_error)?;
        let proofs_publisher = node
            .create_dynamic_publisher(message_type.clone(), config.proofs_topic.as_str())
            .map_err(rclrs_error)?;

        let queued_tasks = Arc::new(Mutex::new(VecDeque::new()));
        let callback_queue = Arc::clone(&queued_tasks);
        let tasks_subscription = node
            .create_dynamic_subscription(
                message_type,
                config.tasks_topic.as_str(),
                move |message: DynamicMessage, _info| match task_from_message(&message) {
                    Ok(Some(task)) => {
                        if let Ok(mut queue) = callback_queue.lock() {
                            queue.push_back(task);
                        }
                    }
                    Ok(None) => {}
                    Err(error) => {
                        eprintln!("stellar ROS bridge: dropping task message: {error}");
                    }
                },
            )
            .map_err(rclrs_error)?;

        let executor_commands = Arc::clone(ros_executor.commands());
        let spin_thread = thread::Builder::new()
            .name("stellar-ros-bridge-rclrs".to_owned())
            .spawn(move || {
                let errors = ros_executor.spin(SpinOptions::default());
                for error in errors {
                    eprintln!("stellar ROS bridge: executor stopped with error: {error}");
                }
            })
            .map_err(|error| {
                TransportError::new(format!("failed to start the ROS executor thread: {error}"))
            })?;

        Ok(Self {
            capabilities_publisher,
            proofs_publisher,
            _tasks_subscription: tasks_subscription,
            metadata,
            _node: node,
            _context: context,
            executor_commands,
            spin_thread: Some(spin_thread),
            queued_tasks,
            executor,
        })
    }

    fn publish_json<T: serde::Serialize>(
        &self,
        publisher: &DynamicPublisher,
        payload: &T,
    ) -> Result<(), TransportError> {
        let data = serde_json::to_string(payload)
            .map_err(|error| TransportError::new(format!("serializing message failed: {error}")))?;
        let mut message = self.metadata.create().map_err(rclrs_error)?;
        set_string_field(&mut message, "data", &data)?;
        publisher.publish(message).map_err(rclrs_error)
    }
}

impl<E> Transport for RclrsTransport<E>
where
    E: TaskExecutor,
{
    fn advertise(&self, capability: &Capability) -> Result<(), TransportError> {
        self.publish_json(&self.capabilities_publisher, capability)
    }

    fn next_task(&self) -> Result<Option<Task>, TransportError> {
        Ok(self
            .queued_tasks
            .lock()
            .map_err(|_| TransportError::new("task queue lock poisoned"))?
            .pop_front())
    }

    fn execute(&self, task: &Task) -> Result<Proof, TransportError> {
        self.executor.execute(task)
    }

    fn publish_proof(&self, proof: &Proof) -> Result<(), TransportError> {
        self.publish_json(&self.proofs_publisher, proof)
    }
}

impl<E> Drop for RclrsTransport<E> {
    fn drop(&mut self) {
        self.executor_commands.halt_spinning();
        if let Some(handle) = self.spin_thread.take() {
            let _ = handle.join();
        }
    }
}

fn message_type() -> MessageTypeName {
    MessageTypeName {
        package_name: "std_msgs".to_owned(),
        type_name: "String".to_owned(),
    }
}

fn set_string_field(
    message: &mut DynamicMessage,
    field: &str,
    value: &str,
) -> Result<(), TransportError> {
    let Some(ValueMut::Simple(SimpleValueMut::String(data))) = message.get_mut(field) else {
        return Err(TransportError::new(format!(
            "std_msgs/msg/String has no string field {field:?}"
        )));
    };
    *data = RosString::from(value);
    Ok(())
}

fn task_from_message(message: &DynamicMessage) -> Result<Option<Task>, TransportError> {
    let Some(Value::Simple(SimpleValue::String(data))) = message.get("data") else {
        return Ok(None);
    };
    let task = serde_json::from_str::<Task>(&data.to_string()).map_err(|error| {
        TransportError::new(format!("invalid Task JSON on tasks topic: {error}"))
    })?;
    Ok(Some(task))
}

fn rclrs_error(error: impl std::fmt::Display) -> TransportError {
    TransportError::new(format!("ROS 2 error: {error}"))
}
