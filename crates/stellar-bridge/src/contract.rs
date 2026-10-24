//! Pure request construction, result parsing, and contract-error mapping.
//!
//! Nothing in this module performs I/O, which makes the TaskEscrow ABI easy to
//! test without a network.

use std::str::FromStr;

use bridge_core::{AccountId, AssetId, SettlementError, Task, TaskId, TaskState};
use soroban_client::address::{Address, AddressTrait};
use soroban_client::xdr::{BytesM, Int128Parts, ScBytes, ScMap, ScSymbol, ScVal, ScVec, StringM};

/// Error values declared by `contracts/task-escrow`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContractError {
    InvalidAmount,
    InvalidDeadline,
    NotFound,
    InvalidStatus,
    Unauthorized,
    DeadlinePassed,
    DeadlineNotPassed,
}

impl ContractError {
    /// Numeric value returned by the contract.
    pub fn code(self) -> u32 {
        match self {
            Self::InvalidAmount => 1,
            Self::InvalidDeadline => 2,
            Self::NotFound => 3,
            Self::InvalidStatus => 4,
            Self::Unauthorized => 5,
            Self::DeadlinePassed => 6,
            Self::DeadlineNotPassed => 7,
        }
    }

    /// Decode the contract's numeric error code.
    pub fn from_code(code: u32) -> Option<Self> {
        Some(match code {
            1 => Self::InvalidAmount,
            2 => Self::InvalidDeadline,
            3 => Self::NotFound,
            4 => Self::InvalidStatus,
            5 => Self::Unauthorized,
            6 => Self::DeadlinePassed,
            7 => Self::DeadlineNotPassed,
            _ => return None,
        })
    }

    /// Human-readable meaning of the code.
    pub fn description(self) -> &'static str {
        match self {
            Self::InvalidAmount => "reward amount must be positive",
            Self::InvalidDeadline => "deadline must be in the future",
            Self::NotFound => "task not found",
            Self::InvalidStatus => "task is not in a state that allows this operation",
            Self::Unauthorized => "the signing account is not authorized for this operation",
            Self::DeadlinePassed => "the task deadline has already passed",
            Self::DeadlineNotPassed => "the task deadline has not passed",
        }
    }
}

/// Extract a contract error code from a Soroban error string such as
/// `HostError: Error(Contract, #5)`.
pub fn parse_contract_error(text: &str) -> Option<ContractError> {
    let marker = "Error(Contract, #";
    let start = text.find(marker)? + marker.len();
    let digits = text[start..]
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>();
    if digits.is_empty() {
        return None;
    }
    digits.parse().ok().and_then(ContractError::from_code)
}

/// Map a raw Soroban/RPC error string to a domain error without inventing codes.
pub fn map_contract_error(text: &str) -> SettlementError {
    match parse_contract_error(text) {
        Some(error) => SettlementError::new(format!(
            "TaskEscrow error #{} ({}): {text}",
            error.code(),
            error.description()
        )),
        None => SettlementError::new(text),
    }
}

/// A single TaskEscrow invocation. Keeping this enum pure makes it possible to
/// assert on every argument before any RPC call happens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContractCall {
    CreateTask {
        requester: AccountId,
        capability: String,
        token: AccountId,
        amount: i128,
        deadline: u64,
    },
    AcceptTask {
        task_id: TaskId,
        executor: AccountId,
    },
    SubmitProof {
        task_id: TaskId,
        executor: AccountId,
        digest: [u8; 32],
    },
    Release {
        task_id: TaskId,
    },
    Refund {
        task_id: TaskId,
    },
    GetTask {
        task_id: TaskId,
    },
}

impl ContractCall {
    /// Build a `create_task` call using `token` for the reward asset.
    pub fn create_task(task: &Task, token: AccountId) -> Self {
        Self::CreateTask {
            requester: task.requester.clone(),
            capability: task.capability.as_str().to_owned(),
            token,
            amount: task.reward.amount,
            deadline: task.deadline,
        }
    }

    /// Contract entry point name.
    pub fn function_name(&self) -> &'static str {
        match self {
            Self::CreateTask { .. } => "create_task",
            Self::AcceptTask { .. } => "accept_task",
            Self::SubmitProof { .. } => "submit_proof",
            Self::Release { .. } => "release",
            Self::Refund { .. } => "refund",
            Self::GetTask { .. } => "get_task",
        }
    }

    /// Encode the arguments exactly as the contract expects them.
    pub fn args(&self) -> Result<Vec<ScVal>, SettlementError> {
        match self {
            Self::CreateTask {
                requester,
                capability,
                token,
                amount,
                deadline,
            } => Ok(vec![
                address(requester.as_str())?,
                symbol(capability)?,
                address(token.as_str())?,
                int128(*amount),
                ScVal::U64(*deadline),
            ]),
            Self::AcceptTask { task_id, executor } => {
                Ok(vec![ScVal::U64(task_id.0), address(executor.as_str())?])
            }
            Self::SubmitProof {
                task_id,
                executor,
                digest,
            } => Ok(vec![
                ScVal::U64(task_id.0),
                address(executor.as_str())?,
                bytes(digest)?,
            ]),
            Self::Release { task_id } | Self::Refund { task_id } | Self::GetTask { task_id } => {
                Ok(vec![ScVal::U64(task_id.0)])
            }
        }
    }
}

/// Resolve the Stellar Asset Contract used for a reward.
pub fn token_contract(
    asset: &AssetId,
    native_token_contract: Option<&AccountId>,
) -> Result<AccountId, SettlementError> {
    let token = match asset {
        AssetId::Sac(id) => id.clone(),
        AssetId::Native => native_token_contract.cloned().ok_or_else(|| {
            SettlementError::new(
                "native XLM rewards need STELLAR_TOKEN_CONTRACT_ID (the XLM Stellar Asset Contract)",
            )
        })?,
    };
    if !token.as_str().starts_with('C') {
        return Err(SettlementError::new(
            "reward token must be a Stellar Asset Contract id starting with C",
        ));
    }
    Ok(token)
}

/// Parse the `status` field of the TaskEscrow `Task` struct.
///
/// Soroban encodes a struct as an `ScVal::Map`, a unit enum variant as a
/// one-element `ScVal::Vec`, and `Option::Some` as a one-element vector. All
/// three wrappers are accepted here so the parser also works if the contract
/// later exposes a status-only view.
pub fn parse_task_state(value: &ScVal) -> Option<TaskState> {
    match value {
        ScVal::Void => None,
        ScVal::Vec(Some(ScVec(items))) => {
            if items.len() == 1 {
                let inner = &items[0];
                if matches!(inner, ScVal::Map(_)) {
                    parse_task_state(inner)
                } else {
                    state_from_status(inner)
                }
            } else {
                None
            }
        }
        ScVal::Map(Some(ScMap(entries))) => entries.iter().find_map(|entry| {
            if symbol_text(&entry.key).as_deref() == Some("status") {
                state_from_status(&entry.val)
            } else {
                None
            }
        }),
        _ => state_from_status(value),
    }
}

fn state_from_status(value: &ScVal) -> Option<TaskState> {
    let status = match value {
        ScVal::Vec(Some(ScVec(items))) if items.len() == 1 => symbol_text(&items[0])?,
        other => symbol_text(other)?,
    };
    Some(match status.as_str() {
        "Open" => TaskState::Requested,
        "Accepted" => TaskState::Accepted,
        "ProofSubmitted" => TaskState::ProofSubmitted,
        "Released" => TaskState::Settled,
        "Refunded" => TaskState::Expired,
        _ => return None,
    })
}

fn symbol_text(value: &ScVal) -> Option<String> {
    match value {
        ScVal::Symbol(ScSymbol(text)) => text.to_utf8_string().ok(),
        ScVal::String(text) => text.0.to_utf8_string().ok(),
        _ => None,
    }
}

fn address(value: &str) -> Result<ScVal, SettlementError> {
    Address::new(value)
        .map_err(|error| SettlementError::new(format!("invalid Stellar address {value}: {error}")))?
        .to_sc_val()
        .map_err(|error| SettlementError::new(format!("invalid Stellar address {value}: {error}")))
}

fn symbol(value: &str) -> Result<ScVal, SettlementError> {
    let valid = !value.is_empty()
        && value.len() <= 32
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_');
    if !valid {
        return Err(SettlementError::new(format!(
            "capability {value:?} cannot be encoded as a Soroban Symbol (max 32 ASCII alphanumeric/underscore characters)"
        )));
    }
    let text = StringM::<32>::from_str(value)
        .map_err(|error| SettlementError::new(format!("invalid Soroban Symbol: {error}")))?;
    Ok(ScVal::Symbol(ScSymbol(text)))
}

fn int128(value: i128) -> ScVal {
    ScVal::I128(Int128Parts {
        hi: (value >> 64) as i64,
        lo: value as u64,
    })
}

fn bytes(digest: &[u8; 32]) -> Result<ScVal, SettlementError> {
    let bytes = BytesM::try_from(digest.to_vec())
        .map_err(|error| SettlementError::new(format!("invalid proof digest: {error}")))?;
    Ok(ScVal::Bytes(ScBytes(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bridge_core::{CapabilityId, Proof, Reward};
    use soroban_client::xdr::{ScMapEntry, StringM};

    const REQUESTER: &str = "GBBM6BKZPEHWYO3E3YKREDPQXMS4VK35YLNU7NFBRI26RAN7GI5POFBB";
    const EXECUTOR: &str = "GA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJVSGZ";
    const CONTRACT: &str = "CA3D5KRYM6CB7OWQ6TWYRR3Z4T7GNZLKERYNZGGA5SOAOPIFY6YQGAXE";

    fn task() -> Task {
        Task {
            id: TaskId(9),
            capability: CapabilityId::new("navigate").unwrap(),
            params: serde_json::json!({"x": 1}),
            reward: Reward::new(AssetId::Native, 100).unwrap(),
            deadline: 2_000_000,
            requester: AccountId::new(REQUESTER).unwrap(),
        }
    }

    fn symbol_value(value: &str) -> ScVal {
        ScVal::Symbol(ScSymbol(StringM::<32>::from_str(value).unwrap()))
    }

    #[test]
    fn create_task_arguments_match_the_contract_abi() {
        let call = ContractCall::create_task(&task(), AccountId::new(CONTRACT).unwrap());
        assert_eq!(call.function_name(), "create_task");
        let args = call.args().unwrap();
        assert_eq!(args.len(), 5);

        assert!(matches!(&args[0], ScVal::Address(_)));
        assert_eq!(args[1], symbol_value("navigate"));
        assert!(matches!(&args[2], ScVal::Address(_)));
        let ScVal::I128(parts) = &args[3] else {
            panic!("amount is not i128");
        };
        let amount = (i128::from(parts.hi) << 64) | i128::from(parts.lo);
        assert_eq!(amount, 100);
        assert_eq!(args[4], ScVal::U64(2_000_000));
    }

    #[test]
    fn accept_and_proof_arguments_use_the_contract_order() {
        let accept = ContractCall::AcceptTask {
            task_id: TaskId(9),
            executor: AccountId::new(EXECUTOR).unwrap(),
        };
        let accept_args = accept.args().unwrap();
        assert_eq!(accept_args[0], ScVal::U64(9));
        assert!(matches!(&accept_args[1], ScVal::Address(_)));

        let digest = [7u8; 32];
        let proof = ContractCall::SubmitProof {
            task_id: TaskId(9),
            executor: AccountId::new(EXECUTOR).unwrap(),
            digest,
        };
        let proof_args = proof.args().unwrap();
        assert_eq!(proof_args[0], ScVal::U64(9));
        assert!(matches!(&proof_args[1], ScVal::Address(_)));
        let ScVal::Bytes(bytes) = &proof_args[2] else {
            panic!("digest is not bytes");
        };
        assert_eq!(bytes.0.as_ref(), digest);
    }

    #[test]
    fn release_refund_and_get_task_take_only_the_id() {
        for call in [
            ContractCall::Release { task_id: TaskId(4) },
            ContractCall::Refund { task_id: TaskId(4) },
            ContractCall::GetTask { task_id: TaskId(4) },
        ] {
            assert_eq!(call.args().unwrap(), vec![ScVal::U64(4)]);
        }
    }

    #[test]
    fn proof_helper_matches_the_domain_proof() {
        let proof = Proof {
            task_id: TaskId(1),
            digest: [1u8; 32],
            signer: AccountId::new(EXECUTOR).unwrap(),
            signature: [0u8; 64],
        };
        let call = ContractCall::SubmitProof {
            task_id: proof.task_id,
            executor: proof.signer.clone(),
            digest: proof.digest,
        };
        assert_eq!(call.args().unwrap()[2], bytes(&[1u8; 32]).unwrap());
    }

    #[test]
    fn capability_must_be_symbol_safe() {
        let mut task = task();
        task.capability = CapabilityId::new("navigate-indoors").unwrap();
        let call = ContractCall::create_task(&task, AccountId::new(CONTRACT).unwrap());
        assert!(call.args().is_err());
    }

    #[test]
    fn native_asset_requires_a_configured_contract() {
        let asset = AssetId::Native;
        assert!(token_contract(&asset, None).is_err());
        let token = AccountId::new(CONTRACT).unwrap();
        assert_eq!(token_contract(&asset, Some(&token)).unwrap(), token);
    }

    #[test]
    fn parses_status_from_a_contract_task_map() {
        let status = ScVal::Vec(Some(ScVec(
            vec![symbol_value("Released")].try_into().unwrap(),
        )));
        let task_value = ScVal::Map(Some(ScMap(
            vec![ScMapEntry {
                key: symbol_value("status"),
                val: status,
            }]
            .try_into()
            .unwrap(),
        )));

        assert_eq!(parse_task_state(&task_value), Some(TaskState::Settled));
    }

    #[test]
    fn parses_option_wrapped_task_and_void() {
        let status = ScVal::Vec(Some(ScVec(vec![symbol_value("Open")].try_into().unwrap())));
        let task_value = ScVal::Map(Some(ScMap(
            vec![ScMapEntry {
                key: symbol_value("status"),
                val: status,
            }]
            .try_into()
            .unwrap(),
        )));
        let some = ScVal::Vec(Some(ScVec(vec![task_value].try_into().unwrap())));

        assert_eq!(parse_task_state(&some), Some(TaskState::Requested));
        assert_eq!(parse_task_state(&ScVal::Void), None);
    }

    #[test]
    fn parses_every_contract_status() {
        for (name, expected) in [
            ("Open", TaskState::Requested),
            ("Accepted", TaskState::Accepted),
            ("ProofSubmitted", TaskState::ProofSubmitted),
            ("Released", TaskState::Settled),
            ("Refunded", TaskState::Expired),
        ] {
            let value = ScVal::Vec(Some(ScVec(vec![symbol_value(name)].try_into().unwrap())));
            assert_eq!(parse_task_state(&value), Some(expected), "{name}");
        }
    }

    #[test]
    fn maps_all_contract_error_codes() {
        for (code, expected) in [
            (1, ContractError::InvalidAmount),
            (2, ContractError::InvalidDeadline),
            (3, ContractError::NotFound),
            (4, ContractError::InvalidStatus),
            (5, ContractError::Unauthorized),
            (6, ContractError::DeadlinePassed),
            (7, ContractError::DeadlineNotPassed),
        ] {
            let text = format!("HostError: Error(Contract, #{code})");
            assert_eq!(parse_contract_error(&text), Some(expected));
            let mapped = map_contract_error(&text).to_string();
            assert!(mapped.contains(expected.description()), "{mapped}");
        }
        assert_eq!(parse_contract_error("Error(Contract, #99)"), None);
        assert_eq!(parse_contract_error("network down"), None);
    }
}
