//! Domain types shared by the ROS transport, the settlement client and the node.

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

/// Identifier assigned by the escrow contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TaskId(pub u64);

impl std::fmt::Display for TaskId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

/// Stellar account or contract identifier (strkey-encoded).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AccountId(String);

impl AccountId {
    /// Validate and wrap a Stellar strkey (`G...` account or `C...` contract).
    pub fn new(value: impl Into<String>) -> Result<Self, CoreError> {
        let value = value.into();
        let valid_len = value.len() == 56;
        let valid_prefix = matches!(value.chars().next(), Some('G') | Some('C'));
        let valid_chars = value
            .chars()
            .all(|ch| ch.is_ascii_uppercase() || ('2'..='7').contains(&ch));
        if !valid_len || !valid_prefix || !valid_chars {
            return Err(CoreError::InvalidAccountId(value));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for AccountId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A robot service advertised on the ROS graph, e.g. `navigate` or `pick`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CapabilityId(String);

impl CapabilityId {
    pub fn new(value: impl Into<String>) -> Result<Self, CoreError> {
        let value = value.into();
        let valid = !value.is_empty()
            && value.len() <= 64
            && value
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'));
        if !valid {
            return Err(CoreError::InvalidCapabilityId(value));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for CapabilityId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Asset used to pay for a task.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AssetId {
    /// Native XLM.
    Native,
    /// Stellar Asset Contract address for a issued asset.
    Sac(AccountId),
}

/// Reward attached to a task.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reward {
    pub asset: AssetId,
    pub amount: i128,
}

impl Reward {
    pub fn new(asset: AssetId, amount: i128) -> Result<Self, CoreError> {
        if amount <= 0 {
            return Err(CoreError::InvalidReward(amount));
        }
        Ok(Self { asset, amount })
    }
}

/// A capability advertisement published by a robot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capability {
    pub id: CapabilityId,
    pub version: u32,
    /// Human-readable unit, e.g. `per_metre` or `per_item`.
    pub units: String,
    pub metadata: serde_json::Value,
}

/// A unit of paid work requested from a robot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub capability: CapabilityId,
    pub params: serde_json::Value,
    pub reward: Reward,
    /// Unix timestamp (ledger time) after which the task may be refunded.
    pub deadline: u64,
    pub requester: AccountId,
}

/// Evidence that a task was completed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proof {
    pub task_id: TaskId,
    /// SHA-256 digest of the signed telemetry payload.
    #[serde(with = "hex_array")]
    pub digest: [u8; 32],
    pub signer: AccountId,
    #[serde(with = "hex_array")]
    pub signature: [u8; 64],
}

/// Serde adapter: fixed-size byte arrays are encoded as lowercase hex strings.
mod hex_array {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S, const N: usize>(value: &[u8; N], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&encode(value))
    }

    pub fn deserialize<'de, D, const N: usize>(deserializer: D) -> Result<[u8; N], D::Error>
    where
        D: Deserializer<'de>,
    {
        let text = String::deserialize(deserializer)?;
        decode(&text).map_err(serde::de::Error::custom)
    }

    fn encode(bytes: &[u8]) -> String {
        use std::fmt::Write;
        let mut text = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            write!(text, "{byte:02x}").expect("writing to a String cannot fail");
        }
        text
    }

    fn decode<const N: usize>(text: &str) -> Result<[u8; N], String> {
        if text.len() != N * 2 {
            return Err(format!("expected {} hex characters", N * 2));
        }
        let mut bytes = [0u8; N];
        for (index, chunk) in text.as_bytes().chunks(2).enumerate() {
            let pair = std::str::from_utf8(chunk).map_err(|error| error.to_string())?;
            bytes[index] = u8::from_str_radix(pair, 16).map_err(|error| error.to_string())?;
        }
        Ok(bytes)
    }
}
