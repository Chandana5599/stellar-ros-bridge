#![no_std]
//! TaskEscrow: holds a task reward until the robot proves completion.
//!
//! Lifecycle: `Open -> Accepted -> ProofSubmitted -> Released`, or
//! `Open/Accepted -> Refunded` once the deadline has passed.

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, token, Address, BytesN,
    Env, Symbol,
};

pub type TaskId = u64;

const TTL_THRESHOLD: u32 = 120_960;
const TTL_EXTEND_TO: u32 = 518_400;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    InvalidAmount = 1,
    InvalidDeadline = 2,
    NotFound = 3,
    InvalidStatus = 4,
    Unauthorized = 5,
    DeadlinePassed = 6,
    DeadlineNotPassed = 7,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Status {
    Open,
    Accepted,
    ProofSubmitted,
    Released,
    Refunded,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Task {
    pub requester: Address,
    pub executor: Option<Address>,
    pub capability: Symbol,
    pub token: Address,
    pub amount: i128,
    pub deadline: u64,
    pub status: Status,
    pub proof_digest: BytesN<32>,
}

#[contracttype]
pub enum DataKey {
    Task(TaskId),
    NextId,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskCreated {
    #[topic]
    pub task_id: TaskId,
    pub requester: Address,
    pub capability: Symbol,
    pub amount: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskAccepted {
    #[topic]
    pub task_id: TaskId,
    pub executor: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProofSubmitted {
    #[topic]
    pub task_id: TaskId,
    pub executor: Address,
    pub digest: BytesN<32>,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskReleased {
    #[topic]
    pub task_id: TaskId,
    pub executor: Address,
    pub amount: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskRefunded {
    #[topic]
    pub task_id: TaskId,
    pub requester: Address,
    pub amount: i128,
}

#[contract]
pub struct TaskEscrow;

#[contractimpl]
impl TaskEscrow {
    pub fn create_task(
        env: Env,
        requester: Address,
        capability: Symbol,
        token: Address,
        amount: i128,
        deadline: u64,
    ) -> Result<TaskId, Error> {
        requester.require_auth();
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        if deadline <= env.ledger().timestamp() {
            return Err(Error::InvalidDeadline);
        }

        token::Client::new(&env, &token).transfer(
            &requester,
            env.current_contract_address(),
            &amount,
        );

        let next: TaskId = env
            .storage()
            .persistent()
            .get(&DataKey::NextId)
            .unwrap_or(0);
        let task_id = next + 1;
        env.storage().persistent().set(&DataKey::NextId, &task_id);

        let task = Task {
            requester: requester.clone(),
            executor: None,
            capability: capability.clone(),
            token,
            amount,
            deadline,
            status: Status::Open,
            proof_digest: BytesN::from_array(&env, &[0u8; 32]),
        };
        save(&env, task_id, &task);
        TaskCreated {
            task_id,
            requester,
            capability,
            amount,
        }
        .publish(&env);
        Ok(task_id)
    }

    pub fn accept_task(env: Env, task_id: TaskId, executor: Address) -> Result<(), Error> {
        executor.require_auth();
        let mut task = load(&env, task_id)?;
        if task.status != Status::Open {
            return Err(Error::InvalidStatus);
        }
        if task.requester == executor {
            return Err(Error::Unauthorized);
        }
        if env.ledger().timestamp() >= task.deadline {
            return Err(Error::DeadlinePassed);
        }
        task.executor = Some(executor.clone());
        task.status = Status::Accepted;
        save(&env, task_id, &task);
        TaskAccepted { task_id, executor }.publish(&env);
        Ok(())
    }

    pub fn submit_proof(
        env: Env,
        task_id: TaskId,
        executor: Address,
        digest: BytesN<32>,
    ) -> Result<(), Error> {
        executor.require_auth();
        let mut task = load(&env, task_id)?;
        if task.status != Status::Accepted {
            return Err(Error::InvalidStatus);
        }
        if task.executor != Some(executor.clone()) {
            return Err(Error::Unauthorized);
        }
        if env.ledger().timestamp() >= task.deadline {
            return Err(Error::DeadlinePassed);
        }
        task.proof_digest = digest.clone();
        task.status = Status::ProofSubmitted;
        save(&env, task_id, &task);
        ProofSubmitted {
            task_id,
            executor,
            digest,
        }
        .publish(&env);
        Ok(())
    }

    pub fn release(env: Env, task_id: TaskId) -> Result<(), Error> {
        let mut task = load(&env, task_id)?;
        if task.status != Status::ProofSubmitted {
            return Err(Error::InvalidStatus);
        }
        task.requester.require_auth();
        let executor = task.executor.clone().ok_or(Error::Unauthorized)?;

        task.status = Status::Released;
        save(&env, task_id, &task);
        token::Client::new(&env, &task.token).transfer(
            &env.current_contract_address(),
            &executor,
            &task.amount,
        );
        TaskReleased {
            task_id,
            executor,
            amount: task.amount,
        }
        .publish(&env);
        Ok(())
    }

    pub fn refund(env: Env, task_id: TaskId) -> Result<(), Error> {
        let mut task = load(&env, task_id)?;
        if task.status != Status::Open && task.status != Status::Accepted {
            return Err(Error::InvalidStatus);
        }
        if env.ledger().timestamp() < task.deadline {
            return Err(Error::DeadlineNotPassed);
        }
        task.requester.require_auth();
        let requester = task.requester.clone();

        task.status = Status::Refunded;
        save(&env, task_id, &task);
        token::Client::new(&env, &task.token).transfer(
            &env.current_contract_address(),
            &requester,
            &task.amount,
        );
        TaskRefunded {
            task_id,
            requester,
            amount: task.amount,
        }
        .publish(&env);
        Ok(())
    }

    pub fn get_task(env: Env, task_id: TaskId) -> Option<Task> {
        env.storage().persistent().get(&DataKey::Task(task_id))
    }
}

fn load(env: &Env, task_id: TaskId) -> Result<Task, Error> {
    let key = DataKey::Task(task_id);
    let task: Task = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(Error::NotFound)?;
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    Ok(task)
}

fn save(env: &Env, task_id: TaskId, task: &Task) {
    let key = DataKey::Task(task_id);
    env.storage().persistent().set(&key, task);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    env.storage()
        .persistent()
        .extend_ttl(&DataKey::NextId, TTL_THRESHOLD, TTL_EXTEND_TO);
}

#[cfg(test)]
mod test;
