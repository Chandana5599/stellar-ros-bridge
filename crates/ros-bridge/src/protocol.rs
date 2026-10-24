//! rosbridge_suite JSON frames and JSON payload decoding.
//!
//! This module is intentionally independent of the WebSocket connection so the
//! wire format can be tested without a running ROS graph.

use bridge_core::{Task, TransportError};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// The message type used on every bridge topic.
pub const MESSAGE_TYPE: &str = "std_msgs/msg/String";

/// Frame that advertises a publisher on a rosbridge server.
pub fn advertise(topic: &str) -> String {
    json!({
        "op": "advertise",
        "topic": topic,
        "type": MESSAGE_TYPE,
    })
    .to_string()
}

/// Frame that subscribes to a rosbridge topic.
pub fn subscribe(topic: &str) -> String {
    json!({
        "op": "subscribe",
        "topic": topic,
        "type": MESSAGE_TYPE,
    })
    .to_string()
}

/// Frame that publishes a JSON payload in the `data` field of a String message.
pub fn publish<T: Serialize>(topic: &str, payload: &T) -> Result<String, TransportError> {
    let data = serde_json::to_string(payload)
        .map_err(|error| TransportError::new(format!("serializing message failed: {error}")))?;
    Ok(json!({
        "op": "publish",
        "topic": topic,
        "msg": { "data": data },
    })
    .to_string())
}

#[derive(Debug, Deserialize)]
struct IncomingFrame {
    op: String,
    #[serde(default)]
    msg: Option<IncomingMessage>,
}

#[derive(Debug, Deserialize)]
struct IncomingMessage {
    #[serde(default)]
    data: Option<String>,
}

/// Extract a [`Task`] from an incoming rosbridge frame.
///
/// Returns `Ok(None)` for frames that are not a `publish` with a String
/// payload. A `publish` frame whose `data` is not a valid [`Task`] is an error:
/// silently dropping malformed paid work would be unsafe.
pub fn parse_task_frame(text: &str) -> Result<Option<Task>, TransportError> {
    let frame: IncomingFrame = serde_json::from_str(text)
        .map_err(|error| TransportError::new(format!("invalid rosbridge frame: {error}")))?;
    if frame.op != "publish" {
        return Ok(None);
    }
    let Some(data) = frame.msg.and_then(|message| message.data) else {
        return Ok(None);
    };
    let task = serde_json::from_str::<Task>(&data).map_err(|error| {
        TransportError::new(format!("invalid Task JSON on tasks topic: {error}"))
    })?;
    Ok(Some(task))
}

/// Render a JSON value as a compact string, mainly useful for diagnostics.
pub fn encode(value: &Value) -> String {
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bridge_core::{AccountId, AssetId, CapabilityId, Reward, TaskId};
    use serde_json::json;

    fn task() -> Task {
        Task {
            id: TaskId(7),
            capability: CapabilityId::new("navigate").unwrap(),
            params: json!({"x": 1.0, "y": 2.0}),
            reward: Reward::new(AssetId::Native, 100).unwrap(),
            deadline: 2_000_000,
            requester: AccountId::new(format!("G{}", "A".repeat(55))).unwrap(),
        }
    }

    #[test]
    fn advertise_uses_the_string_type() {
        let frame: Value = serde_json::from_str(&advertise("/robot/capabilities")).unwrap();
        assert_eq!(frame["op"], "advertise");
        assert_eq!(frame["topic"], "/robot/capabilities");
        assert_eq!(frame["type"], MESSAGE_TYPE);
    }

    #[test]
    fn publish_wraps_payload_in_msg_data() {
        let task = task();
        let frame: Value = serde_json::from_str(&publish("/robot/tasks", &task).unwrap()).unwrap();

        assert_eq!(frame["op"], "publish");
        assert_eq!(frame["topic"], "/robot/tasks");
        let data = frame["msg"]["data"].as_str().unwrap();
        let decoded: Task = serde_json::from_str(data).unwrap();
        assert_eq!(decoded, task);
    }

    #[test]
    fn task_frames_round_trip() {
        let task = task();
        let inner = serde_json::to_string(&task).unwrap();
        let frame = json!({
            "op": "publish",
            "topic": "/robot/tasks",
            "msg": {"data": inner},
        })
        .to_string();

        assert_eq!(parse_task_frame(&frame).unwrap(), Some(task));
    }

    #[test]
    fn non_publish_frames_are_ignored() {
        assert_eq!(
            parse_task_frame(r#"{"op":"service_response","msg":{"data":"x"}}"#).unwrap(),
            None
        );
    }

    #[test]
    fn malformed_task_json_is_an_error() {
        let frame = json!({
            "op": "publish",
            "topic": "/robot/tasks",
            "msg": {"data": "{\"not\":\"a task\"}"},
        })
        .to_string();

        assert!(parse_task_frame(&frame).is_err());
    }
}
