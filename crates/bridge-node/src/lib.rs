//! Runtime library for Stellar ROS Bridge.
//!
//! [`driver::BridgeNode`] wires a ROS transport to Stellar settlement and
//! drives the accept -> execute -> prove -> settle lifecycle.

use ros_bridge::config::NodeConfig;
use std::error::Error;
use std::path::Path;

/// The task lifecycle driver lives in `bridge-core` so it can be tested
/// without ROS 2 or Stellar dependencies.
pub use bridge_core::driver;
/// Health and readiness endpoints for the node.
pub mod health;

/// The crate version, sourced from `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Load node configuration from a parameter file, or use documented defaults.
pub fn load_config(params_file: Option<&Path>) -> Result<NodeConfig, Box<dyn Error>> {
    match params_file {
        Some(path) => NodeConfig::from_parameter_file(path),
        None => Ok(NodeConfig::default()),
    }
}
