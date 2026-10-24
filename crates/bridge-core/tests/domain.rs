use bridge_core::{
    AccountId, AssetId, CapabilityId, CoreError, Reward, Task, TaskEvent, TaskId, TaskState,
};
use serde_json::json;

fn account(prefix: char) -> AccountId {
    AccountId::new(format!("{prefix}{}", "A".repeat(55))).expect("valid strkey shape")
}

fn task() -> Task {
    Task {
        id: TaskId(1),
        capability: CapabilityId::new("navigate").unwrap(),
        params: json!({"x": 1.5, "y": 2.5}),
        reward: Reward::new(AssetId::Native, 100).unwrap(),
        deadline: 1_700_000_000,
        requester: account('G'),
    }
}

#[test]
fn accepts_account_and_contract_ids() {
    assert!(AccountId::new(format!("G{}", "A".repeat(55))).is_ok());
    assert!(AccountId::new(format!("C{}", "A".repeat(55))).is_ok());
}

#[test]
fn rejects_malformed_account_ids() {
    for value in [
        "".to_string(),
        "GABC".to_string(),
        format!("X{}", "A".repeat(55)),
        format!("G{}", "a".repeat(55)),
        format!("G{}", "1".repeat(55)),
    ] {
        assert!(AccountId::new(value).is_err());
    }
}

#[test]
fn rejects_malformed_capability_ids() {
    assert!(CapabilityId::new("").is_err());
    assert!(CapabilityId::new("has space").is_err());
    assert!(CapabilityId::new("x".repeat(65)).is_err());
    assert!(CapabilityId::new("navigate_v2").is_ok());
}

#[test]
fn rejects_non_positive_rewards() {
    assert_eq!(
        Reward::new(AssetId::Native, 0),
        Err(CoreError::InvalidReward(0))
    );
    assert!(Reward::new(AssetId::Native, -5).is_err());
}

#[test]
fn happy_path_moves_through_every_state() {
    let mut state = TaskState::Announced;
    for event in [
        TaskEvent::Request,
        TaskEvent::Accept,
        TaskEvent::Start,
        TaskEvent::SubmitProof,
        TaskEvent::Settle,
    ] {
        state = state.transition(event).unwrap();
    }
    assert_eq!(state, TaskState::Settled);
    assert!(state.is_terminal());
}

#[test]
fn dispute_and_expiry_paths() {
    assert_eq!(
        TaskState::Accepted.transition(TaskEvent::Dispute).unwrap(),
        TaskState::Disputed
    );
    assert_eq!(
        TaskState::InProgress.transition(TaskEvent::Expire).unwrap(),
        TaskState::Expired
    );
}

#[test]
fn illegal_transitions_are_rejected() {
    let error = TaskState::Announced
        .transition(TaskEvent::Settle)
        .unwrap_err();
    assert!(matches!(error, CoreError::InvalidTransition { .. }));

    assert!(TaskState::Settled.transition(TaskEvent::Settle).is_err());
    assert!(TaskState::ProofSubmitted
        .transition(TaskEvent::Expire)
        .is_err());
}

#[test]
fn tasks_round_trip_through_json() {
    let original = task();
    let encoded = serde_json::to_string(&original).unwrap();
    let decoded: Task = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, original);
}
