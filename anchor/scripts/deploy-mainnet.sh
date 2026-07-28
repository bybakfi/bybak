#!/usr/bin/env bash
# Bybak -- MAINNET deploy. Never run automatically; it is a deliberate manual step.
#
# A green devnet run does not imply mainnet approval. Those are separate decisions.
#
#   export HELIUS_RPC_URL="https://mainnet.helius-rpc.com/?api-key=<key>"
#   ./scripts/deploy-mainnet.sh
#
# The RPC key is read from the environment and is never hardcoded in this file.

set -euo pipefail

EXPECTED_PUBKEY="BmnQou5hLWSvhw4CY5TzEzWVfz7UANyhAaGRc4YTQcqD"
PROGRAM_ID="8n1BA3TB1tfYzU75GR9CDePXZEeoXXEYQVEs3QqwTRrj"
KEYPAIR_PATH="${HOME}/bybak-deploy.json"
PROGRAM_KEYPAIR="target/deploy/bybak-keypair.json"
SO_PATH="target/deploy/bybak.so"
# Program rent measured on devnet was 1.867 SOL; keep headroom for fees + retries.
MIN_BALANCE_SOL="2.5"

echo "=== Bybak MAINNET deploy ==="

# --- preflight ----------------------------------------------------------------
[ -f "$KEYPAIR_PATH" ]    || { echo "FAIL: missing keypair $KEYPAIR_PATH"; exit 1; }
[ -f "$PROGRAM_KEYPAIR" ] || { echo "FAIL: missing program keypair $PROGRAM_KEYPAIR -- run 'anchor build' first"; exit 1; }
[ -f "$SO_PATH" ]         || { echo "FAIL: missing $SO_PATH -- run 'anchor build' first"; exit 1; }

DERIVED="$(solana-keygen pubkey "$KEYPAIR_PATH")"
if [ "$DERIVED" != "$EXPECTED_PUBKEY" ]; then
  echo "FAIL: PUBKEY MISMATCH"
  echo "  declared: $EXPECTED_PUBKEY"
  echo "  derived : $DERIVED"
  exit 1
fi

DERIVED_PROGRAM="$(solana address -k "$PROGRAM_KEYPAIR")"
if [ "$DERIVED_PROGRAM" != "$PROGRAM_ID" ]; then
  echo "FAIL: program keypair does not match the declared program id"
  echo "  declared: $PROGRAM_ID"
  echo "  derived : $DERIVED_PROGRAM"
  echo "  -> declare_id! and the program keypair have drifted apart. ABORT."
  exit 1
fi

# Large program deploys get rate-limited on the public mainnet RPC.
if [ -z "${HELIUS_RPC_URL:-}" ]; then
  echo "WARN: HELIUS_RPC_URL not set. The public mainnet RPC rate-limits large"
  echo "      program deploys and will likely fail partway through."
  RPC="https://api.mainnet-beta.solana.com"
else
  RPC="$HELIUS_RPC_URL"
fi

BAL_RAW="$(solana balance "$DERIVED" --url "$RPC")"
BAL_NUM="$(echo "$BAL_RAW" | awk '{print $1}')"

echo "  address     : $DERIVED"
echo "  program id  : $PROGRAM_ID"
echo "  binary      : $SO_PATH ($(wc -c < "$SO_PATH") bytes)"
echo "  cluster     : mainnet-beta"
echo "  rpc         : $(echo "$RPC" | sed 's/api-key=.*/api-key=***/')"
echo "  balance     : $BAL_RAW"
echo "  need approx : ${MIN_BALANCE_SOL} SOL"

if awk "BEGIN{exit !($BAL_NUM < $MIN_BALANCE_SOL)}"; then
  echo "FAIL: balance below ${MIN_BALANCE_SOL} SOL. Fund the wallet and retry. ABORT."
  exit 1
fi

# --- explicit confirmation ----------------------------------------------------
echo ""
echo "This spends REAL SOL on mainnet-beta and is not reversible."
printf 'Type exactly DEPLOY MAINNET to proceed: '
read -r CONFIRM
if [ "$CONFIRM" != "DEPLOY MAINNET" ]; then
  echo "Aborted."
  exit 1
fi

# solana program deploy is more reliable than anchor deploy for large binaries.
solana program deploy "$SO_PATH" \
  --program-id "$PROGRAM_KEYPAIR" \
  --keypair "$KEYPAIR_PATH" \
  --url "$RPC"

echo ""
echo "Deployed. Publishing IDL..."
anchor idl init -f target/idl/bybak.json "$PROGRAM_ID" \
  --provider.cluster "$RPC" \
  --provider.wallet "$KEYPAIR_PATH" \
  || echo "WARN: IDL init failed (program itself is deployed). Retry: anchor idl init ..."

echo ""
echo "Explorer: https://explorer.solana.com/address/${PROGRAM_ID}"
solana balance "$DERIVED" --url "$RPC"
