//! Runtime configuration for the live Stellar/Soroban client.
//!
//! Secrets are read from the environment (or supplied by the embedding
//! application) and are never serialized or included in `Debug` output.

use std::env;
use std::str::FromStr;
use std::time::Duration;

use bridge_core::{AccountId, CoreError};
use soroban_client::network::{NetworkPassphrase, Networks};
use stellar_strkey::ed25519::PrivateKey;

/// Environment variable holding the source account's secret key.
pub const SECRET_ENV: &str = "STELLAR_SECRET";
/// Environment variable holding the deployed TaskEscrow contract id.
pub const CONTRACT_ENV: &str = "TASK_ESCROW_CONTRACT_ID";

/// A secret string whose `Debug` representation is always redacted.
#[derive(Clone)]
pub struct SecretString(String);

impl SecretString {
    /// Wrap a secret value.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Access the raw secret. Callers must not log the returned value.
    pub fn expose_secret(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for SecretString {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SecretString(***)")
    }
}

/// Configuration needed to submit TaskEscrow transactions.
#[derive(Clone)]
pub struct StellarConfig {
    pub rpc_url: String,
    /// Stellar network passphrase (not the short network name).
    pub network_passphrase: String,
    pub contract_id: AccountId,
    pub source_secret: SecretString,
    /// Stellar Asset Contract id used when a task pays in native XLM.
    pub native_token_contract: Option<AccountId>,
    /// Base inclusion fee in stroops.
    pub fee: u32,
    /// How long to wait for transaction confirmation.
    pub transaction_timeout: Duration,
    /// RPC request timeout in seconds.
    pub rpc_timeout: u64,
    /// Allow plain `http://` RPC URLs, intended for a local sandbox only.
    pub allow_http: bool,
}

impl std::fmt::Debug for StellarConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StellarConfig")
            .field("rpc_url", &self.rpc_url)
            .field("network_passphrase", &self.network_passphrase)
            .field("contract_id", &self.contract_id)
            .field("source_secret", &self.source_secret)
            .field("native_token_contract", &self.native_token_contract)
            .field("fee", &self.fee)
            .field("transaction_timeout", &self.transaction_timeout)
            .field("rpc_timeout", &self.rpc_timeout)
            .field("allow_http", &self.allow_http)
            .finish()
    }
}

impl StellarConfig {
    /// Load configuration from environment variables.
    ///
    /// Required: `STELLAR_SECRET`, `TASK_ESCROW_CONTRACT_ID`.
    ///
    /// Optional: `STELLAR_RPC_URL` (defaults to testnet),
    /// `STELLAR_NETWORK` (defaults to `testnet`; a raw passphrase is accepted),
    /// `STELLAR_TOKEN_CONTRACT_ID`, `STELLAR_FEE`, `STELLAR_TIMEOUT_SECONDS`,
    /// `STELLAR_RPC_TIMEOUT_SECONDS`, and `STELLAR_ALLOW_HTTP`.
    pub fn from_env() -> Result<Self, ConfigError> {
        let rpc_url = optional("STELLAR_RPC_URL")
            .unwrap_or_else(|| "https://soroban-testnet.stellar.org".to_owned());
        let network = optional("STELLAR_NETWORK").unwrap_or_else(|| "testnet".to_owned());
        let network_passphrase = passphrase_for_network(&network);

        let source_secret = SecretString::new(required(SECRET_ENV)?);
        validate_secret(&source_secret)?;

        let contract_id = parse_contract_id(CONTRACT_ENV, &required(CONTRACT_ENV)?)?;
        let native_token_contract = optional("STELLAR_TOKEN_CONTRACT_ID")
            .map(|value| parse_contract_id("STELLAR_TOKEN_CONTRACT_ID", &value))
            .transpose()?;

        let fee = optional("STELLAR_FEE")
            .map(|value| parse_number("STELLAR_FEE", &value))
            .transpose()?
            .unwrap_or(100);
        let transaction_timeout = Duration::from_secs(
            optional("STELLAR_TIMEOUT_SECONDS")
                .map(|value| parse_number("STELLAR_TIMEOUT_SECONDS", &value))
                .transpose()?
                .unwrap_or(30),
        );
        let rpc_timeout = optional("STELLAR_RPC_TIMEOUT_SECONDS")
            .map(|value| parse_number("STELLAR_RPC_TIMEOUT_SECONDS", &value))
            .transpose()?
            .unwrap_or(30);
        let allow_http = optional("STELLAR_ALLOW_HTTP")
            .map(|value| parse_bool("STELLAR_ALLOW_HTTP", &value))
            .transpose()?
            .unwrap_or(false);

        Ok(Self {
            rpc_url,
            network_passphrase,
            contract_id,
            source_secret,
            native_token_contract,
            fee,
            transaction_timeout,
            rpc_timeout,
            allow_http,
        })
    }
}

/// Map a short network name to its Stellar passphrase. Unknown values are
/// treated as a raw passphrase so private networks can be configured too.
pub fn passphrase_for_network(network: &str) -> String {
    match network.trim().to_ascii_lowercase().as_str() {
        "testnet" => Networks::testnet().to_owned(),
        "mainnet" | "public" => Networks::public().to_owned(),
        "futurenet" => Networks::futurenet().to_owned(),
        "sandbox" => Networks::sandbox().to_owned(),
        "standalone" | "local" => Networks::standalone().to_owned(),
        _ => network.trim().to_owned(),
    }
}

/// Validate a Stellar secret seed without ever including it in an error.
pub fn validate_secret(secret: &SecretString) -> Result<(), ConfigError> {
    PrivateKey::from_str(secret.expose_secret())
        .map(|_| ())
        .map_err(|_| ConfigError::InvalidSecret)
}

fn required(name: &'static str) -> Result<String, ConfigError> {
    optional(name).ok_or(ConfigError::Missing(name))
}

fn optional(name: &'static str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.trim().is_empty())
}

fn parse_contract_id(name: &'static str, value: &str) -> Result<AccountId, ConfigError> {
    let id = AccountId::new(value).map_err(|error| invalid(name, error))?;
    if !id.as_str().starts_with('C') {
        return Err(ConfigError::Invalid {
            name,
            reason: "expected a contract id starting with C".to_owned(),
        });
    }
    Ok(id)
}

fn parse_number<T>(name: &'static str, value: &str) -> Result<T, ConfigError>
where
    T: FromStr,
{
    value.parse().map_err(|_| ConfigError::Invalid {
        name,
        reason: "expected a number".to_owned(),
    })
}

fn parse_bool(name: &'static str, value: &str) -> Result<bool, ConfigError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" => Ok(true),
        "0" | "false" | "no" => Ok(false),
        _ => Err(ConfigError::Invalid {
            name,
            reason: "expected true/false".to_owned(),
        }),
    }
}

fn invalid(name: &'static str, error: CoreError) -> ConfigError {
    ConfigError::Invalid {
        name,
        reason: error.to_string(),
    }
}

/// Configuration problems. Messages never contain secret material.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("missing environment variable {0}")]
    Missing(&'static str),
    #[error("invalid {name}: {reason}")]
    Invalid { name: &'static str, reason: String },
    #[error("STELLAR_SECRET is not a valid Stellar secret key")]
    InvalidSecret,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_names_map_to_passphrases() {
        assert_eq!(
            passphrase_for_network("testnet"),
            "Test SDF Network ; September 2015"
        );
        assert_eq!(
            passphrase_for_network("MAINNET"),
            "Public Global Stellar Network ; September 2015"
        );
        assert_eq!(passphrase_for_network("Local Sandbox"), "Local Sandbox");
    }

    #[test]
    fn secret_debug_is_redacted() {
        let secret = SecretString::new("S".to_owned() + &"A".repeat(55));
        let rendered = format!("{secret:?}");
        assert_eq!(rendered, "SecretString(***)");
        assert!(!rendered.contains("AAAA"));
    }

    #[test]
    fn config_debug_does_not_leak_the_secret() {
        let config = StellarConfig {
            rpc_url: "https://example.invalid".to_owned(),
            network_passphrase: "passphrase".to_owned(),
            contract_id: AccountId::new(format!("C{}", "A".repeat(55))).unwrap(),
            source_secret: SecretString::new("S".to_owned() + &"B".repeat(55)),
            native_token_contract: None,
            fee: 100,
            transaction_timeout: Duration::from_secs(30),
            rpc_timeout: 10,
            allow_http: false,
        };
        let rendered = format!("{config:?}");
        assert!(!rendered.contains(&"B".repeat(55)));
        assert!(rendered.contains("SecretString(***)"));
    }

    #[test]
    fn invalid_secret_is_rejected_without_echoing_it() {
        let secret = SecretString::new("not-a-secret");
        let error = validate_secret(&secret).unwrap_err().to_string();
        assert!(!error.contains("not-a-secret"));
    }
}
