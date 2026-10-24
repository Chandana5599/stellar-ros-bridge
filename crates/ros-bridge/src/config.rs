//! Configuration parsed from a ROS 2 parameter file.
//!
//! This module is independent of a live ROS 2 runtime so configuration can
//! be tested on machines without ROS installed.

use serde::Deserialize;
use serde_yaml::Value;
use std::error::Error;
use std::fs;
use std::io;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct NodeConfig {
    pub capabilities_topic: String,
    pub tasks_topic: String,
    pub proofs_topic: String,
    pub ros_domain_id: u32,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            capabilities_topic: "/robot/capabilities".to_owned(),
            tasks_topic: "/robot/tasks".to_owned(),
            proofs_topic: "/robot/proofs".to_owned(),
            ros_domain_id: 0,
        }
    }
}

impl NodeConfig {
    /// Load parameters from a ROS 2 YAML parameter file.
    ///
    /// The file must contain a node entry with a `ros__parameters` mapping.
    /// Parameters omitted from that mapping retain their defaults.
    pub fn from_parameter_file(path: impl AsRef<Path>) -> Result<Self, Box<dyn Error>> {
        let contents = fs::read_to_string(path)?;
        Self::from_parameter_yaml(&contents)
    }

    /// Parse a ROS 2 YAML parameter document.
    pub fn from_parameter_yaml(yaml: &str) -> Result<Self, Box<dyn Error>> {
        let document: Value = serde_yaml::from_str(yaml)?;

        let parameters_key = Value::String("ros__parameters".to_owned());
        let parameters = document
            .as_mapping()
            .and_then(|root| {
                root.values().find_map(|node| {
                    node.as_mapping()
                        .and_then(|mapping| mapping.get(&parameters_key))
                })
            })
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "ROS 2 parameter file is missing a ros__parameters mapping",
                )
            })?;

        let config: Self = serde_yaml::from_value(parameters.clone())?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), Box<dyn Error>> {
        for (name, topic) in [
            ("capabilities_topic", &self.capabilities_topic),
            ("tasks_topic", &self.tasks_topic),
            ("proofs_topic", &self.proofs_topic),
        ] {
            if topic.trim().is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("{name} must not be empty"),
                )
                .into());
            }
        }

        // ROS 2's commonly supported domain ID range is 0 through 232.
        if self.ros_domain_id > 232 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "ros_domain_id must be between 0 and 232",
            )
            .into());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::NodeConfig;

    #[test]
    fn defaults_match_documented_topics_and_domain() {
        let config = NodeConfig::default();

        assert_eq!(config.capabilities_topic, "/robot/capabilities");
        assert_eq!(config.tasks_topic, "/robot/tasks");
        assert_eq!(config.proofs_topic, "/robot/proofs");
        assert_eq!(config.ros_domain_id, 0);
    }

    #[test]
    fn yaml_overrides_are_applied() {
        let yaml = r#"
"/bridge_node":
  ros__parameters:
    capabilities_topic: /custom/capabilities
    tasks_topic: /custom/tasks
    proofs_topic: /custom/proofs
    ros_domain_id: 42
"#;

        let config = NodeConfig::from_parameter_yaml(yaml).unwrap();

        assert_eq!(config.capabilities_topic, "/custom/capabilities");
        assert_eq!(config.tasks_topic, "/custom/tasks");
        assert_eq!(config.proofs_topic, "/custom/proofs");
        assert_eq!(config.ros_domain_id, 42);
    }

    #[test]
    fn omitted_parameters_keep_their_defaults() {
        let yaml = r#"
"/bridge_node":
  ros__parameters:
    tasks_topic: /custom/tasks
"#;

        let config = NodeConfig::from_parameter_yaml(yaml).unwrap();

        assert_eq!(config.capabilities_topic, "/robot/capabilities");
        assert_eq!(config.tasks_topic, "/custom/tasks");
        assert_eq!(config.proofs_topic, "/robot/proofs");
        assert_eq!(config.ros_domain_id, 0);
    }

    #[test]
    fn invalid_domain_id_is_rejected() {
        let yaml = r#"
"/bridge_node":
  ros__parameters:
    ros_domain_id: 233
"#;

        assert!(NodeConfig::from_parameter_yaml(yaml).is_err());
    }

    #[test]
    fn empty_topic_is_rejected() {
        let yaml = r#"
"/bridge_node":
  ros__parameters:
    tasks_topic: ""
"#;

        assert!(NodeConfig::from_parameter_yaml(yaml).is_err());
    }

    #[test]
    fn missing_ros_parameters_mapping_is_rejected() {
        let yaml = r#"
"/bridge_node":
  another_section:
    tasks_topic: /custom/tasks
"#;

        assert!(NodeConfig::from_parameter_yaml(yaml).is_err());
    }
}
