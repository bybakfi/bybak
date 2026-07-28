#!/usr/bin/env bash
# safe_solana_tx.sh -- mandatory gate in front of every SOL-spending command.
#
# Verifies, before anything touches the chain:
#   1. the keypair file exists,
#   2. the pubkey derived from it matches the pubkey declared in this script,
#   3. the target cluster is one of the three this script knows about,
#   4. the balance is above a floor.
#
# A pubkey mismatch means the wrong keypair is in place. Abort, always.
#
# Usage:  ./scripts/safe_solana_tx.sh <cluster> "<command to run>"
#   e.g.  ./scripts/safe_solana_tx.sh devnet "anchor deploy --provider.cluster devnet"

set -euo pipefail

EXPECTED_PUBKEY="BmnQou5hLWSvhw4CY5TzEzWVfz7UANyhAaGRc4YTQcqD"
KEYPAIR_PATH="${HOME}/bybak-deploy.json"
MIN_BALANCE_SOL="0.5"

CLUSTER="${1:-}"
CMD="${2:-}"

[ -n "$CLUSTER" ] || { echo "FAIL: cluster not given (localnet|devnet|mainnet)"; exit 1; }
[ -n "$CMD" ]     || { echo "FAIL: command not given"; exit 1; }
[ -f "$KEYPAIR_PATH" ] || { echo "FAIL: keypair file missing: $KEYPAIR_PATH"; exit 1; }

# --- 1. pubkey identity check -------------------------------------------------
ACTUAL_PUBKEY="$(solana-keygen pubkey "$KEYPAIR_PATH")"
if [ "$ACTUAL_PUBKEY" != "$EXPECTED_PUBKEY" ]; then
  echo "FAIL: keypair pubkey mismatch"
  echo "  declared: $EXPECTED_PUBKEY"
  echo "  derived : $ACTUAL_PUBKEY"
  echo "  -> wrong keypair for this program. ABORT."
  exit 1
fi

# --- 2. cluster -> RPC --------------------------------------------------------
case "$CLUSTER" in
  localnet) RPC="http://localhost:8899" ;;
  devnet)   RPC="https://api.devnet.solana.com" ;;
  mainnet)  RPC="${HELIUS_RPC_URL:-https://api.mainnet-beta.solana.com}" ;;
  *) echo "FAIL: unknown cluster: $CLUSTER"; exit 1 ;;
esac

# --- 3. balance floor ---------------------------------------------------------
BAL_RAW="$(solana balance "$ACTUAL_PUBKEY" --url "$RPC")"
BAL_NUM="$(echo "$BAL_RAW" | awk '{print $1}')"
if awk "BEGIN{exit !($BAL_NUM < $MIN_BALANCE_SOL)}"; then
  echo "FAIL: balance $BAL_RAW below floor ${MIN_BALANCE_SOL} SOL on $CLUSTER. ABORT."
  exit 1
fi

echo "--- funds safety verify PASS ---"
echo "  address : $ACTUAL_PUBKEY"
echo "  cluster : $CLUSTER ($RPC)"
echo "  balance : $BAL_RAW"
echo "  command : $CMD"
echo "--------------------------------"

eval "$CMD"
