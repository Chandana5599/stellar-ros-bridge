//! Live Soroban settlement client.
//!
//! The client uses `soroban-client` for transaction construction, simulation,
//! signing and submission, and layers the domain [`Settlement`] trait on top.
//! All network work goes through the `Server` RPC boundary, so unit tests can
//! exercise request construction and error mapping without making calls.

use std::future::Future;
use std::time::Duration;

use bridge_core::{AccountId, Proof, Settlement, SettlementError, Task, TaskId, TaskState};
use soroban_client::contract::{ContractBehavior, Contracts};
use soroban_client::error::Error as SdkError;
use soroban_client::keypair::{Keypair, KeypairBehavior};
use soroban_client::soroban_rpc::{SendTransactionStatus, TransactionStatus};
use soroban_client::transaction::{
    Transaction, TransactionBehavior, TransactionBuilder, TransactionBuilderBehavior,
};
use soroban_client::xdr::ScVal;
use soroban_client::{Options, Server};
use tokio::runtime::{Builder as RuntimeBuilder, Handle as RuntimeHandle, Runtime};

use crate::config::{validate_secret, StellarConfig};
use crate::contract::{map_contract_error, parse_task_state, token_contract, ContractCall};

/// Settings retained after the secret has been moved into a signing keypair.
#[derive(Debug, Clone)]
struct Settings {
    network_passphrase: String,
    native_token_contract: Option<AccountId>,
    fee: u32,
    transaction_timeout: Duration,
}

/// A [`Settlement`] implementation backed by a live Stellar RPC endpoint.
///
/// One instance represents one Stellar account. Use separate instances for the
/// requester and executor: the TaskEscrow contract authorizes `create_task`,
/// `release` and `refund` as the requester and `accept_task`/`submit_proof` as
/// the executor.
pub struct StellarSettlement {
    rpc: Server,
    contract: Contracts,
    keypair: Keypair,
    public_key: String,
    settings: Settings,
    runtime: Runtime,
}

impl std::fmt::Debug for StellarSettlement {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StellarSettlement")
            .field("public_key", &self.public_key)
            .field("settings", &self.settings)
            .finish_non_exhaustive()
    }
}

impl StellarSettlement {
    /// Create a client. This performs no network I/O; the first RPC call happens
    /// when a [`Settlement`] method is invoked.
    pub fn new(config: StellarConfig) -> Result<Self, SettlementError> {
        validate_secret(&config.source_secret).map_err(|_| {
            SettlementError::new("STELLAR_SECRET is not a valid Stellar secret key")
        })?;
        let keypair = Keypair::from_secret(config.source_secret.expose_secret())
            .map_err(|_| SettlementError::new("could not load the Stellar signing key"))?;
        let public_key = keypair.public_key();

        let rpc = Server::new(
            &config.rpc_url,
            Options {
                allow_http: config.allow_http,
                timeout: config.rpc_timeout,
                ..Options::default()
            },
        )
        .map_err(|error| {
            SettlementError::new(format!("invalid Stellar RPC configuration: {error}"))
        })?;

        let contract = Contracts::new(config.contract_id.as_str()).map_err(|error| {
            SettlementError::new(format!("invalid TaskEscrow contract id: {error}"))
        })?;

        let runtime = RuntimeBuilder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .map_err(|error| {
                SettlementError::new(format!("could not start the Stellar RPC runtime: {error}"))
            })?;

        Ok(Self {
            rpc,
            contract,
            keypair,
            public_key,
            settings: Settings {
                network_passphrase: config.network_passphrase,
                native_token_contract: config.native_token_contract,
                fee: config.fee,
                transaction_timeout: config.transaction_timeout,
            },
            runtime,
        })
    }

    /// The Stellar account (G...) used to sign transactions.
    pub fn public_key(&self) -> &str {
        &self.public_key
    }

    async fn unsigned(&self, call: &ContractCall) -> Result<Transaction, SettlementError> {
        let mut account = self
            .rpc
            .get_account(&self.public_key)
            .await
            .map_err(map_sdk_error)?;
        let operation = self.contract.call(call.function_name(), Some(call.args()?));

        let mut builder =
            TransactionBuilder::new(&mut account, &self.settings.network_passphrase, None);
        builder.fee(self.settings.fee);
        builder
            .set_timeout(
                self.settings
                    .transaction_timeout
                    .as_secs()
                    .min(i64::MAX as u64) as i64,
            )
            .map_err(|error| {
                SettlementError::new(format!("invalid transaction timeout: {error}"))
            })?;
        builder.add_operation(operation);
        Ok(builder.build())
    }

    async fn simulate(&self, call: &ContractCall) -> Result<ScVal, SettlementError> {
        let transaction = self.unsigned(call).await?;
        let response = self
            .rpc
            .simulate_transaction(&transaction, None)
            .await
            .map_err(map_sdk_error)?;
        if let Some(error) = response.error.as_deref() {
            return Err(map_contract_error(error));
        }
        response
            .to_result()
            .map(|(value, _auth)| value)
            .ok_or_else(|| {
                SettlementError::new(format!(
                    "{} simulation returned no result",
                    call.function_name()
                ))
            })
    }

    async fn submit(&self, call: &ContractCall) -> Result<Option<ScVal>, SettlementError> {
        let transaction = self.unsigned(call).await?;
        let mut prepared = self
            .rpc
            .prepare_transaction(&transaction)
            .await
            .map_err(map_sdk_error)?;
        prepared.sign(std::slice::from_ref(&self.keypair));

        let sent = self
            .rpc
            .send_transaction(prepared)
            .await
            .map_err(map_sdk_error)?;

        if sent.status == SendTransactionStatus::Error {
            let detail = sent
                .to_error_result()
                .map(|result| format!("{result:?}"))
                .unwrap_or_else(|| "transaction rejected by the RPC".to_owned());
            return Err(map_contract_error(&detail));
        }
        if sent.status == SendTransactionStatus::TryAgainLater {
            return Err(SettlementError::new(
                "Stellar RPC is overloaded; retry the transaction later",
            ));
        }

        let result = self
            .rpc
            .wait_transaction(&sent.hash, self.settings.transaction_timeout)
            .await
            .map_err(|(error, _last)| map_sdk_error(error))?;

        match result.status {
            TransactionStatus::Success => Ok(result
                .to_result_meta()
                .and_then(|(_meta, return_value)| return_value)),
            TransactionStatus::Failed => {
                let result_detail = result
                    .to_result()
                    .map(|value| format!("{value:?}"))
                    .unwrap_or_default();
                let diagnostics = result
                    .to_diagnostic_events()
                    .map(|events| format!("{events:?}"))
                    .unwrap_or_default();
                Err(map_contract_error(&format!(
                    "transaction failed: {result_detail} {diagnostics}"
                )))
            }
            TransactionStatus::NotFound => Err(SettlementError::new(
                "transaction was not found before the confirmation timeout",
            )),
        }
    }

    fn block_on<F, T>(&self, future: F) -> T
    where
        F: Future<Output = T> + Send,
        T: Send,
    {
        if RuntimeHandle::try_current().is_ok() {
            std::thread::scope(
                |scope| match scope.spawn(|| self.runtime.block_on(future)).join() {
                    Ok(value) => value,
                    Err(panic) => std::panic::resume_unwind(panic),
                },
            )
        } else {
            self.runtime.block_on(future)
        }
    }

    fn executor_account(&self) -> AccountId {
        AccountId::new(self.public_key.clone())
            .expect("the keypair always returns a valid Stellar account id")
    }
}

impl Settlement for StellarSettlement {
    fn create_task(&self, task: &Task) -> Result<TaskId, SettlementError> {
        let token = token_contract(
            &task.reward.asset,
            self.settings.native_token_contract.as_ref(),
        )?;
        let call = ContractCall::create_task(task, token);
        let returned = self.block_on(self.submit(&call))?;
        match returned {
            Some(ScVal::U64(id)) => Ok(TaskId(id)),
            Some(other) => Err(SettlementError::new(format!(
                "create_task returned an unexpected value: {other:?}"
            ))),
            None => Err(SettlementError::new(
                "create_task succeeded but the RPC did not return a task id",
            )),
        }
    }

    fn accept_task(&self, id: TaskId, executor: &AccountId) -> Result<(), SettlementError> {
        if executor != &self.executor_account() {
            return Err(SettlementError::new(
                "accept_task must be signed by the executor account",
            ));
        }
        let call = ContractCall::AcceptTask {
            task_id: id,
            executor: executor.clone(),
        };
        self.block_on(self.submit(&call)).map(|_| ())
    }

    fn submit_proof(&self, id: TaskId, proof: &Proof) -> Result<(), SettlementError> {
        if proof.task_id != id {
            return Err(SettlementError::new(format!(
                "proof belongs to task {}, expected {id}",
                proof.task_id
            )));
        }
        if proof.signer.as_str() != self.public_key {
            return Err(SettlementError::new(
                "proof signer does not match the settlement account",
            ));
        }
        let call = ContractCall::SubmitProof {
            task_id: id,
            executor: self.executor_account(),
            digest: proof.digest,
        };
        self.block_on(self.submit(&call)).map(|_| ())
    }

    fn release(&self, id: TaskId) -> Result<(), SettlementError> {
        self.block_on(self.submit(&ContractCall::Release { task_id: id }))
            .map(|_| ())
    }

    fn refund(&self, id: TaskId) -> Result<(), SettlementError> {
        self.block_on(self.submit(&ContractCall::Refund { task_id: id }))
            .map(|_| ())
    }

    fn get_state(&self, id: TaskId) -> Result<TaskState, SettlementError> {
        let value = self.block_on(self.simulate(&ContractCall::GetTask { task_id: id }))?;
        parse_task_state(&value).ok_or_else(|| {
            SettlementError::new(format!(
                "task {id} was not found or has an unknown contract status"
            ))
        })
    }
}

fn map_sdk_error(error: SdkError) -> SettlementError {
    match error {
        SdkError::SimulationFailed(message) | SdkError::RPCError { message, .. }
            if crate::contract::parse_contract_error(&message).is_some() =>
        {
            map_contract_error(&message)
        }
        SdkError::InvalidRpc(error) => {
            SettlementError::new(format!("invalid Stellar RPC URL: {error}"))
        }
        SdkError::XdrError => SettlementError::new("could not encode or decode Stellar XDR"),
        SdkError::JsonError(message) => {
            SettlementError::new(format!("could not parse Stellar RPC response: {message}"))
        }
        SdkError::NetworkError(error) => {
            SettlementError::new(format!("network error while calling Stellar RPC: {error}"))
        }
        SdkError::AccountNotFound => SettlementError::new(
            "source account was not found on this network; fund it with friendbot first",
        ),
        SdkError::ContractDataNotFound => {
            SettlementError::new("TaskEscrow contract data was not found")
        }
        SdkError::ContractCodeNotFound => {
            SettlementError::new("TaskEscrow contract code was not found")
        }
        SdkError::ExternalRefOwnerNotContract => {
            SettlementError::new("contract executable reference has an invalid owner")
        }
        SdkError::InvalidExternalRefTagEntry => {
            SettlementError::new("contract executable reference has an invalid tag")
        }
        SdkError::TransactionError => {
            SettlementError::new("could not build the Stellar transaction")
        }
        SdkError::InvalidSorobanTransaction => {
            SettlementError::new("the RPC did not accept a Soroban transaction")
        }
        SdkError::SimulationFailed(message) => map_contract_error(&message),
        SdkError::RestorationRequired(_fee, _data) => SettlementError::new(
            "TaskEscrow state is archived; restore its footprint before retrying",
        ),
        SdkError::RPCError { code, message } => {
            SettlementError::new(format!("Stellar RPC error {code}: {message}"))
        }
        SdkError::UnexpectedError => {
            SettlementError::new("Stellar RPC returned an unexpected response")
        }
        SdkError::NoFriendbot => SettlementError::new("this network has no friendbot"),
        SdkError::WaitTransactionTimeout(wait, elapsed) => SettlementError::new(format!(
            "timed out after {elapsed}s waiting for the transaction (limit {wait}s)"
        )),
        SdkError::NotImplemented(feature) => {
            SettlementError::new(format!("soroban-client does not implement {feature}"))
        }
    }
}

/// Return the native Stellar Asset Contract id commonly used on testnet.
///
/// This constant is informational only; configure the value explicitly with
/// `STELLAR_TOKEN_CONTRACT_ID` rather than assuming it.
pub const TESTNET_NATIVE_SAC: &str = "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SecretString;

    fn config(secret: &str) -> StellarConfig {
        StellarConfig {
            rpc_url: "https://example.invalid".to_owned(),
            network_passphrase: "Test SDF Network ; September 2015".to_owned(),
            contract_id: AccountId::new("CA3D5KRYM6CB7OWQ6TWYRR3Z4T7GNZLKERYNZGGA5SOAOPIFY6YQGAXE")
                .unwrap(),
            source_secret: SecretString::new(secret),
            native_token_contract: None,
            fee: 100,
            transaction_timeout: Duration::from_secs(1),
            rpc_timeout: 1,
            allow_http: false,
        }
    }

    #[test]
    fn invalid_secret_is_rejected_before_any_network_work() {
        let error = StellarSettlement::new(config("not-a-secret"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("STELLAR_SECRET"));
        assert!(!error.contains("not-a-secret"));
    }
}
