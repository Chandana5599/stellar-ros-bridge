//! ROS 2 integration for Stellar ROS Bridge.
//!
//! Two transports implement [`bridge_core::Transport`]:
//!
//! * [`rosbridge::RosbridgeTransport`] (default `rosbridge` feature) speaks the
//!   [rosbridge_suite](https://github.com/RobotWebTools/rosbridge_suite) JSON
//!   protocol over a WebSocket. It compiles on every platform and needs no ROS
//!   installation in the build environment.
//! * `rclrs_transport::RclrsTransport` (feature `ros`) uses `rclrs` and the
//!   native ROS 2 graph. It requires a sourced ROS 2 Jazzy (or newer)
//!   environment to compile.
//!
//! Both transports exchange `std_msgs/msg/String` messages whose `data` field
//! contains the JSON encoding of the domain types in `bridge_core`.
//!
//! Task execution is deliberately delegated to a [`TaskExecutor`]. A generic
//! transport cannot know how to drive a particular robot, so the default
//! executor returns a clear error instead of fabricating a proof.

pub mod config;
pub mod protocol;

#[cfg(feature = "rosbridge")]
pub mod rosbridge;

#[cfg(feature = "ros")]
pub mod rclrs_transport;

use bridge_core::{Proof, Task, TransportError};

/// Application hook that actually drives the robot for a task.
pub trait TaskExecutor: Send + Sync {
    /// Execute the task and produce proof of completion.
    fn execute(&self, task: &Task) -> Result<Proof, TransportError>;
}

/// Executor used when the application has not supplied one.
///
/// This keeps the transport honest: if execution is not wired up, the
/// lifecycle fails with an actionable error instead of publishing a fake
/// proof.
#[derive(Clone, Copy, Debug, Default)]
pub struct UnsupportedExecutor;

impl TaskExecutor for UnsupportedExecutor {
    fn execute(&self, task: &Task) -> Result<Proof, TransportError> {
        Err(TransportError::new(format!(
            "no TaskExecutor is configured for task {}; wire a robot-specific executor before running the lifecycle",
            task.id
        )))
    }
}

/// Convenience adapter for closures used as task executors.
pub struct FnExecutor<F>(pub F);

impl<F> TaskExecutor for FnExecutor<F>
where
    F: Fn(&Task) -> Result<Proof, TransportError> + Send + Sync,
{
    fn execute(&self, task: &Task) -> Result<Proof, TransportError> {
        (self.0)(task)
    }
}

/// The crate version, sourced from `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
