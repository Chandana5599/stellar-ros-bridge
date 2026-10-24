use bridge_core::driver::BridgeNode;
use bridge_core::mock::{MockClock, MockSettlement, MockTransport};
use bridge_core::{AccountId, AssetId, CapabilityId, Reward, Settlement, Task, TaskId, TaskState};

use serde_json::json;

fn identity() -> AccountId {
    AccountId::new(format!("G{}", "A".repeat(55))).unwrap()
}

fn task(id: u64, deadline: u64) -> Task {
    Task {
        id: TaskId(id),
        capability: CapabilityId::new("navigate").unwrap(),
        params: json!({"x": 1.0, "y": 2.0}),
        reward: Reward::new(AssetId::Native, 50).unwrap(),
        deadline,
        requester: AccountId::new(format!("G{}", "B".repeat(55))).unwrap(),
    }
}

#[test]
fn executes_and_settles_a_task() {
    let settlement = MockSettlement::new();
    let created = settlement.create_task(&task(0, 100)).unwrap();
    let queued = task(created.0, 100);
    let transport = MockTransport::with_tasks(vec![queued]);

    let node = BridgeNode::new(transport, settlement, MockClock::at(10), identity());
    let outcome = node.run_once().unwrap().expect("one queued task");

    assert_eq!(outcome.task_id, created);
    assert_eq!(outcome.state, TaskState::Settled);
    assert_eq!(node.settlement().accepted().len(), 1);
    assert_eq!(node.settlement().submitted().len(), 1);
    assert_eq!(node.settlement().released(), vec![created]);
    assert!(node.settlement().refunded().is_empty());
    assert_eq!(node.transport().executed(), vec![created]);
    assert_eq!(node.transport().published().len(), 1);
    assert_eq!(
        node.settlement().get_state(created).unwrap(),
        TaskState::Settled
    );
}

#[test]
fn refunds_an_expired_task_without_executing_it() {
    let settlement = MockSettlement::new();
    let created = settlement.create_task(&task(0, 100)).unwrap();
    let queued = task(created.0, 100);
    let transport = MockTransport::with_tasks(vec![queued]);

    let node = BridgeNode::new(transport, settlement, MockClock::at(100), identity());
    let outcome = node.run_once().unwrap().expect("one queued task");

    assert_eq!(outcome.state, TaskState::Expired);
    assert_eq!(node.settlement().refunded(), vec![created]);
    assert!(node.settlement().released().is_empty());
    assert!(node.transport().executed().is_empty());
}

#[test]
fn drains_the_queue_up_to_the_limit() {
    let settlement = MockSettlement::new();
    let first = settlement.create_task(&task(0, 100)).unwrap();
    let second = settlement.create_task(&task(0, 100)).unwrap();
    let transport = MockTransport::with_tasks(vec![
        task(first.0, 100),
        task(second.0, 100),
        task(second.0, 100),
    ]);

    let node = BridgeNode::new(transport, settlement, MockClock::at(1), identity());
    let outcomes = node.run_until_idle(2).unwrap();

    assert_eq!(outcomes.len(), 2);
    assert!(outcomes
        .iter()
        .all(|outcome| outcome.state == TaskState::Settled));
    assert_eq!(node.transport().executed().len(), 2);
}

#[test]
fn an_empty_queue_returns_none() {
    let node = BridgeNode::new(
        MockTransport::default(),
        MockSettlement::new(),
        MockClock::at(0),
        identity(),
    );
    assert!(node.run_once().unwrap().is_none());
}
