//! Stellar/Soroban settlement client.
//!
//! The client implements [`bridge_core::Settlement`] against the deployed
//! `TaskEscrow` contract. Request construction and result parsing are pure and
//! unit-tested; network calls only happen when the trait methods are invoked,
//! never from `cargo test`.
//!
//! Secrets come from configuration/environment at runtime. [`StellarConfig`]
//! redacts them from `Debug` output, and errors never include the secret.

#![forbid(unsafe_code)]

pub mod config;
pub mod contract;
pub mod settlement;

pub use config::{passphrase_for_network, SecretString, StellarConfig};
pub use contract::{
    map_contract_error, parse_contract_error, parse_task_state, token_contract, ContractCall,
    ContractError,
};
pub use settlement::{StellarSettlement, TESTNET_NATIVE_SAC};

/// The crate version, sourced from `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
