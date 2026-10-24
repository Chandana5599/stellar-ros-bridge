//! Manual testnet smoke test for the live settlement client.
//!
//! This binary performs real network calls. It is never run by `cargo test`.
//!
//! Required environment:
//!   STELLAR_SECRET, TASK_ESCROW_CONTRACT_ID, STELLAR_TOKEN_CONTRACT_ID,
//!   plus an account funded with the reward asset.
//!
//! Optional environment:
//!   SMOKE_EXECUTOR_SECRET  distinct executor account; when set, the binary
//!                          runs create -> accept -> proof -> release.
//!   SMOKE_CAPABILITY       default `navigate`
//!   SMOKE_AMOUNT           reward in stroops, default `1`
//!   SMOKE_DEADLINE_SECONDS default `120`

use std::error::Error;
use std::time::{SystemTime, UNIX_EPOCH};

use bridge_core::{AccountId, AssetId, CapabilityId, Proof, Reward, Settlement, Task, TaskId};
use serde_json::json;
use stellar_bridge::{SecretString, StellarConfig, StellarSettlement};

fn main() -> Result<(), Box<dyn Error>> {
    let config = StellarConfig::from_env()?;
    let requester = StellarSettlement::new(config.clone())?;
    let requester_id = AccountId::new(requester.public_key())?;

    println!("rpc_url={}", config.rpc_url);
    println!("contract_id={}", config.contract_id);
    println!("requester={}", requester.public_key());

    let capability = CapabilityId::new(env_or("SMOKE_CAPABILITY", "navigate"))?;
    let amount = env_or("SMOKE_AMOUNT", "1").parse::<i128>()?;
    let deadline_seconds = env_or("SMOKE_DEADLINE_SECONDS", "120").parse::<u64>()?;
    let deadline = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() + deadline_seconds;

    let task = Task {
        id: TaskId(0),
        capability,
        params: json!({"smoke": true}),
        reward: Reward::new(AssetId::Native, amount)?,
        deadline,
        requester: requester_id.clone(),
    };

    let id = requester.create_task(&task)?;
    println!("created task {id}");
    println!("state after create: {:?}", requester.get_state(id)?);

    let executor_secret = std::env::var("SMOKE_EXECUTOR_SECRET").ok();
    let Some(executor_secret) = executor_secret else {
        println!(
            "SMOKE_EXECUTOR_SECRET not set; stopping after create/get_state. \
             Set it to a distinct funded account to exercise the full lifecycle."
        );
        return Ok(());
    };

    let mut executor_config = config.clone();
    executor_config.source_secret = SecretString::new(executor_secret);
    let executor = StellarSettlement::new(executor_config)?;
    let executor_id = AccountId::new(executor.public_key())?;
    if executor_id == requester_id {
        return Err("SMOKE_EXECUTOR_SECRET must be a different account from STELLAR_SECRET".into());
    }
    println!("executor={}", executor.public_key());

    executor.accept_task(id, &executor_id)?;
    println!("state after accept: {:?}", requester.get_state(id)?);

    let proof = Proof {
        task_id: id,
        digest: [9u8; 32],
        signer: executor_id,
        signature: [0u8; 64],
    };
    executor.submit_proof(id, &proof)?;
    println!("state after proof: {:?}", requester.get_state(id)?);

    requester.release(id)?;
    println!("state after release: {:?}", requester.get_state(id)?);

    Ok(())
}

fn env_or(name: &str, fallback: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| fallback.to_owned())
}
