//! JSON-over-WebSocket transport for [rosbridge_suite].
//!
//! rosbridge_suite is the standard way to reach a ROS 2 graph without linking
//! against the ROS client libraries. This transport connects to a rosbridge
//! WebSocket server, subscribes to the configured tasks topic, and publishes
//! capabilities and proofs as `std_msgs/msg/String` messages.
//!
//! Only `ws://` endpoints are supported by the default feature set. Enable a
//! TLS feature on `tungstenite` if your deployment needs `wss://`.
//!
//! [rosbridge_suite]: https://github.com/RobotWebTools/rosbridge_suite

use std::collections::VecDeque;
use std::io;
use std::net::TcpStream;
use std::sync::Mutex;
use std::time::Duration;

use bridge_core::{Capability, Proof, Task, Transport, TransportError};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{connect, Error as WsError, Message, WebSocket};

use crate::config::NodeConfig;
use crate::protocol;
use crate::{TaskExecutor, UnsupportedExecutor};

/// How long [`next_task`](Transport::next_task) waits for a frame before
/// reporting an empty queue. Keep this short for a polling robot loop.
pub const DEFAULT_POLL_TIMEOUT: Duration = Duration::from_millis(250);

type Socket = WebSocket<MaybeTlsStream<TcpStream>>;

/// A [`Transport`] backed by a rosbridge_suite WebSocket connection.
pub struct RosbridgeTransport<E = UnsupportedExecutor> {
    config: NodeConfig,
    socket: Mutex<Socket>,
    queued_tasks: Mutex<VecDeque<Task>>,
    executor: E,
    poll_timeout: Duration,
}

impl RosbridgeTransport<UnsupportedExecutor> {
    /// Connect with the default executor, which rejects every task with an
    /// explanatory error. Use [`RosbridgeTransport::with_executor`] in a real
    /// robot deployment.
    pub fn connect(config: NodeConfig, url: impl AsRef<str>) -> Result<Self, TransportError> {
        Self::with_executor(config, url, UnsupportedExecutor)
    }
}

impl<E> RosbridgeTransport<E>
where
    E: TaskExecutor,
{
    /// Connect to `url` (for example `ws://localhost:9090`) and subscribe to
    /// the configured tasks topic.
    pub fn with_executor(
        config: NodeConfig,
        url: impl AsRef<str>,
        executor: E,
    ) -> Result<Self, TransportError> {
        let (mut socket, _response) = connect(url.as_ref()).map_err(ws_error)?;
        set_read_timeout(&mut socket, DEFAULT_POLL_TIMEOUT)?;

        let subscription = protocol::subscribe(&config.tasks_topic);
        socket.send(Message::text(subscription)).map_err(ws_error)?;
        socket.flush().map_err(ws_error)?;

        Ok(Self {
            config,
            socket: Mutex::new(socket),
            queued_tasks: Mutex::new(VecDeque::new()),
            executor,
            poll_timeout: DEFAULT_POLL_TIMEOUT,
        })
    }

    /// Override the polling timeout used by [`Transport::next_task`].
    pub fn set_poll_timeout(&mut self, timeout: Duration) {
        self.poll_timeout = timeout;
        if let Ok(socket) = self.socket.get_mut() {
            let _ = set_read_timeout(socket, timeout);
        }
    }

    fn send_frame(&self, frame: String) -> Result<(), TransportError> {
        let mut socket = self
            .socket
            .lock()
            .map_err(|_| TransportError::new("rosbridge connection lock poisoned"))?;
        socket.send(Message::text(frame)).map_err(ws_error)?;
        socket.flush().map_err(ws_error)
    }

    fn publish_payload<T: serde::Serialize>(
        &self,
        topic: &str,
        payload: &T,
    ) -> Result<(), TransportError> {
        self.send_frame(protocol::publish(topic, payload)?)
    }

    fn read_next_task(&self) -> Result<Option<Task>, TransportError> {
        let mut socket = self
            .socket
            .lock()
            .map_err(|_| TransportError::new("rosbridge connection lock poisoned"))?;

        match socket.read() {
            Ok(Message::Text(text)) => protocol::parse_task_frame(text.as_str()),
            Ok(Message::Binary(bytes)) => {
                let text = std::str::from_utf8(&bytes).map_err(|error| {
                    TransportError::new(format!("non-UTF-8 rosbridge frame: {error}"))
                })?;
                protocol::parse_task_frame(text)
            }
            Ok(Message::Ping(payload)) => {
                socket.send(Message::Pong(payload)).map_err(ws_error)?;
                Ok(None)
            }
            Ok(Message::Close(_)) => Err(TransportError::new(
                "rosbridge server closed the connection",
            )),
            Ok(_) => Ok(None),
            Err(WsError::Io(error)) if is_timeout(&error) => Ok(None),
            Err(error) => Err(ws_error(error)),
        }
    }
}

impl<E> Transport for RosbridgeTransport<E>
where
    E: TaskExecutor,
{
    fn advertise(&self, capability: &Capability) -> Result<(), TransportError> {
        self.send_frame(protocol::advertise(&self.config.capabilities_topic))?;
        self.publish_payload(&self.config.capabilities_topic, capability)
    }

    fn next_task(&self) -> Result<Option<Task>, TransportError> {
        if let Some(task) = self
            .queued_tasks
            .lock()
            .map_err(|_| TransportError::new("task queue lock poisoned"))?
            .pop_front()
        {
            return Ok(Some(task));
        }
        self.read_next_task()
    }

    fn execute(&self, task: &Task) -> Result<Proof, TransportError> {
        self.executor.execute(task)
    }

    fn publish_proof(&self, proof: &Proof) -> Result<(), TransportError> {
        self.publish_payload(&self.config.proofs_topic, proof)
    }
}

fn set_read_timeout(socket: &mut Socket, timeout: Duration) -> Result<(), TransportError> {
    match socket.get_mut() {
        MaybeTlsStream::Plain(stream) => stream.set_read_timeout(Some(timeout)).map_err(|error| {
            TransportError::new(format!("setting socket timeout failed: {error}"))
        }),
        // The default build has no TLS feature. If one is enabled later, the
        // wrapped stream should expose its own timeout hook here.
        _ => Ok(()),
    }
}

fn is_timeout(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
    )
}

fn ws_error(error: WsError) -> TransportError {
    TransportError::new(format!("rosbridge WebSocket error: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bridge_core::{AccountId, AssetId, CapabilityId, Reward, TaskId};
    use serde_json::{json, Value};
    use std::net::TcpListener;

    fn task() -> Task {
        Task {
            id: TaskId(11),
            capability: CapabilityId::new("navigate").unwrap(),
            params: json!({"x": 1.0}),
            reward: Reward::new(AssetId::Native, 10).unwrap(),
            deadline: 3_000_000,
            requester: AccountId::new(format!("G{}", "A".repeat(55))).unwrap(),
        }
    }

    fn capability() -> Capability {
        Capability {
            id: CapabilityId::new("navigate").unwrap(),
            version: 1,
            units: "per_task".to_owned(),
            metadata: json!({}),
        }
    }

    #[test]
    fn default_poll_timeout_is_short() {
        assert!(DEFAULT_POLL_TIMEOUT <= Duration::from_secs(1));
    }

    #[test]
    fn timeout_detection_covers_blocking_and_timed_out_errors() {
        assert!(is_timeout(&io::Error::from(io::ErrorKind::WouldBlock)));
        assert!(is_timeout(&io::Error::from(io::ErrorKind::TimedOut)));
        assert!(!is_timeout(&io::Error::from(
            io::ErrorKind::ConnectionReset
        )));
    }

    #[test]
    fn round_trips_against_a_local_rosbridge_server() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let expected_task = task();
        let expected_capability = capability();
        let expected_proof = Proof {
            task_id: expected_task.id,
            digest: [3u8; 32],
            signer: expected_task.requester.clone(),
            signature: [0u8; 64],
        };

        let server_task = expected_task.clone();
        let server = std::thread::spawn(move || {
            let (stream, _peer) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut socket = tungstenite::accept(stream).unwrap();

            let subscribe = socket.read().unwrap().to_text().unwrap().to_owned();
            let subscribe: Value = serde_json::from_str(&subscribe).unwrap();
            assert_eq!(subscribe["op"], "subscribe");
            assert_eq!(subscribe["type"], protocol::MESSAGE_TYPE);

            let task_frame =
                protocol::publish(subscribe["topic"].as_str().unwrap(), &server_task).unwrap();
            socket.send(Message::text(task_frame)).unwrap();

            let mut received = Vec::new();
            for _ in 0..3 {
                match socket.read() {
                    Ok(message) => received.push(message.to_text().unwrap().to_owned()),
                    Err(_) => break,
                }
            }
            received
        });

        let transport =
            RosbridgeTransport::connect(NodeConfig::default(), format!("ws://{address}")).unwrap();

        assert_eq!(transport.next_task().unwrap(), Some(expected_task.clone()));
        transport.advertise(&expected_capability).unwrap();
        transport.publish_proof(&expected_proof).unwrap();
        drop(transport);

        let frames = server.join().unwrap();
        assert_eq!(frames.len(), 3, "server did not receive all frames");

        let advertise: Value = serde_json::from_str(&frames[0]).unwrap();
        assert_eq!(advertise["op"], "advertise");
        assert_eq!(advertise["type"], protocol::MESSAGE_TYPE);

        let capability_frame: Value = serde_json::from_str(&frames[1]).unwrap();
        assert_eq!(capability_frame["op"], "publish");
        let capability_data: Capability =
            serde_json::from_str(capability_frame["msg"]["data"].as_str().unwrap()).unwrap();
        assert_eq!(capability_data, expected_capability);

        let proof_frame: Value = serde_json::from_str(&frames[2]).unwrap();
        assert_eq!(proof_frame["op"], "publish");
        let proof_data: Proof =
            serde_json::from_str(proof_frame["msg"]["data"].as_str().unwrap()).unwrap();
        assert_eq!(proof_data, expected_proof);
    }
}
