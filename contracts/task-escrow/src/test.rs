use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, Address, BytesN, Env, Symbol,
};

const CAPABILITY: &str = "navigate";

fn setup(env: &Env) -> (Address, TaskEscrowClient<'_>, Address, Address, Address) {
    let contract_id = env.register(TaskEscrow, ());
    let client = TaskEscrowClient::new(env, &contract_id);
    let requester = Address::generate(env);
    let executor = Address::generate(env);
    let admin = Address::generate(env);
    let token_id = env.register_stellar_asset_contract_v2(admin).address();
    (contract_id, client, requester, executor, token_id)
}

fn create(
    client: &TaskEscrowClient<'_>,
    env: &Env,
    requester: &Address,
    token: &Address,
) -> TaskId {
    client.create_task(
        requester,
        &Symbol::new(env, CAPABILITY),
        token,
        &500,
        &(env.ledger().timestamp() + 1_000),
    )
}

#[test]
fn happy_path_releases_reward_once() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client, requester, executor, token_id) = setup(&env);
    let token_client = token::Client::new(&env, &token_id);
    token::StellarAssetClient::new(&env, &token_id).mint(&requester, &1_000);

    let task_id = create(&client, &env, &requester, &token_id);
    assert_eq!(token_client.balance(&contract_id), 500);

    client.accept_task(&task_id, &executor);
    let digest = BytesN::from_array(&env, &[9u8; 32]);
    client.submit_proof(&task_id, &executor, &digest);
    client.release(&task_id);

    assert_eq!(token_client.balance(&executor), 500);
    assert_eq!(token_client.balance(&contract_id), 0);
    let task = client.get_task(&task_id).unwrap();
    assert_eq!(task.status, Status::Released);
    assert_eq!(task.proof_digest, digest);
    assert!(client.try_release(&task_id).is_err());
}

#[test]
fn refund_after_deadline_returns_funds() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, requester, _executor, token_id) = setup(&env);
    let token_client = token::Client::new(&env, &token_id);
    token::StellarAssetClient::new(&env, &token_id).mint(&requester, &500);

    let task_id = create(&client, &env, &requester, &token_id);
    let deadline = client.get_task(&task_id).unwrap().deadline;
    env.ledger().set_timestamp(deadline + 1);

    client.refund(&task_id);

    assert_eq!(token_client.balance(&requester), 500);
    assert_eq!(client.get_task(&task_id).unwrap().status, Status::Refunded);
    assert!(client.try_refund(&task_id).is_err());
}

#[test]
fn refund_before_deadline_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, requester, _executor, token_id) = setup(&env);
    token::StellarAssetClient::new(&env, &token_id).mint(&requester, &500);

    let task_id = create(&client, &env, &requester, &token_id);
    assert!(client.try_refund(&task_id).is_err());
}

#[test]
fn rejects_invalid_amounts_and_past_deadlines() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, requester, _executor, token_id) = setup(&env);
    token::StellarAssetClient::new(&env, &token_id).mint(&requester, &500);
    let capability = Symbol::new(&env, CAPABILITY);

    assert!(client
        .try_create_task(&requester, &capability, &token_id, &0, &100)
        .is_err());
    assert!(client
        .try_create_task(&requester, &capability, &token_id, &500, &0)
        .is_err());
}

#[test]
fn requester_cannot_accept_own_task() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, requester, _executor, token_id) = setup(&env);
    token::StellarAssetClient::new(&env, &token_id).mint(&requester, &500);

    let task_id = create(&client, &env, &requester, &token_id);
    assert!(client.try_accept_task(&task_id, &requester).is_err());
}

#[test]
fn only_the_accepted_executor_can_submit_proof() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, requester, executor, token_id) = setup(&env);
    token::StellarAssetClient::new(&env, &token_id).mint(&requester, &500);
    let stranger = Address::generate(&env);

    let task_id = create(&client, &env, &requester, &token_id);
    client.accept_task(&task_id, &executor);

    let digest = BytesN::from_array(&env, &[1u8; 32]);
    assert!(client
        .try_submit_proof(&task_id, &stranger, &digest)
        .is_err());
    assert!(client
        .try_submit_proof(&task_id, &requester, &digest)
        .is_err());
}

#[test]
fn proof_after_deadline_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, requester, executor, token_id) = setup(&env);
    token::StellarAssetClient::new(&env, &token_id).mint(&requester, &500);

    let task_id = create(&client, &env, &requester, &token_id);
    client.accept_task(&task_id, &executor);
    let deadline = client.get_task(&task_id).unwrap().deadline;
    env.ledger().set_timestamp(deadline);

    let digest = BytesN::from_array(&env, &[2u8; 32]);
    assert!(client
        .try_submit_proof(&task_id, &executor, &digest)
        .is_err());
}

#[test]
fn authorize_create_accept_and_release() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, requester, executor, token_id) = setup(&env);
    token::StellarAssetClient::new(&env, &token_id).mint(&requester, &500);

    // No auth entries: every state-changing call must fail.
    env.set_auths(&[]);
    assert!(client
        .try_create_task(
            &requester,
            &Symbol::new(&env, CAPABILITY),
            &token_id,
            &500,
            &(env.ledger().timestamp() + 1_000),
        )
        .is_err());

    env.mock_all_auths();
    let task_id = create(&client, &env, &requester, &token_id);

    env.set_auths(&[]);
    assert!(client.try_accept_task(&task_id, &executor).is_err());

    env.mock_all_auths();
    client.accept_task(&task_id, &executor);
    let digest = BytesN::from_array(&env, &[3u8; 32]);
    client.submit_proof(&task_id, &executor, &digest);

    env.set_auths(&[]);
    assert!(client.try_release(&task_id).is_err());
}
