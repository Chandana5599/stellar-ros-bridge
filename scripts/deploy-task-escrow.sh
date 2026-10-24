#!/usr/bin/env bash
# Build and deploy the TaskEscrow contract to a Stellar network.
#
# Requires: stellar-cli v25.2.0+, rustup with the wasm32v1-none target, curl.
#
# Usage:
#   STELLAR_NETWORK=testnet scripts/deploy-task-escrow.sh
#   STELLAR_NETWORK=testnet STELLAR_SOURCE=my-identity scripts/deploy-task-escrow.sh
#
# The script creates/funds a throwaway identity when STELLAR_SOURCE does not
# exist. It never prints or stores the secret key.

set -euo pipefail

NETWORK="${STELLAR_NETWORK:-testnet}"
SOURCE="${STELLAR_SOURCE:-task-escrow-deployer}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WASM="$ROOT/contracts/task-escrow/target/wasm32v1-none/release/task_escrow.wasm"

if ! command -v stellar >/dev/null 2>&1; then
  echo "stellar-cli is required: https://developers.stellar.org/docs/tools/cli" >&2
  exit 1
fi

if ! stellar keys address "$SOURCE" >/dev/null 2>&1; then
  echo "creating identity $SOURCE"
  stellar keys generate "$SOURCE" --network "$NETWORK"
fi

ADDRESS="$(stellar keys address "$SOURCE")"
echo "source: $ADDRESS"

if ! curl -fsS "https://friendbot.stellar.org/?addr=$ADDRESS" >/dev/null 2>&1; then
  echo "friendbot skipped (account may already be funded, or this is mainnet)"
fi

cargo build \
  --manifest-path "$ROOT/contracts/task-escrow/Cargo.toml" \
  --release \
  --target wasm32v1-none

CONTRACT_ID="$(stellar contract deploy \
  --wasm "$WASM" \
  --source "$SOURCE" \
  --network "$NETWORK")"

echo "TaskEscrow deployed: $CONTRACT_ID"
echo "export TASK_ESCROW_CONTRACT_ID=$CONTRACT_ID"
echo "explorer: https://stellar.expert/explorer/$NETWORK/contract/$CONTRACT_ID"
