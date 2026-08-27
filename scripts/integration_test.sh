#!/usr/bin/env bash
# VeriTix Soroban CLI integration test against localnet (#682).
#
# Deploys the compiled WASM and verifies core operations through the full
# Soroban execution stack, printing PASS/FAIL with the tx hash per step.
#
# Requirements:
#   - `soroban` CLI installed (https://soroban.stellar.org/docs/reference/soroban-cli)
#   - A localnet (standalone quickstart) reachable at RPC_URL
#   - The contract WASM must exist in target/wasm32v1-none/release/
set -euo pipefail

# --- Configuration (overridable via environment) ---
RPC_URL="${RPC_URL:-http://localhost:8000/soroban/rpc}"
NETWORK_PASSPHRASE="${NETWORK_PASSPHRASE:-Standalone Network ; February 2017}"
ACCOUNT="${STELLAR_ACCOUNT:-integration-test}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WASM="$ROOT/target/wasm32v1-none/release/veritixpay.wasm"

# MIN_ESCROW_AMOUNT from src/storage_types.rs. Keep in sync if it changes.
MIN_ESCROW_AMOUNT=10000000

PASS=0
FAIL=0

pass() { echo "PASS: $1"; PASS=$((PASS + 1)); }
fail() { echo "FAIL: $1"; FAIL=$((FAIL + 1)); }

soroban_args() {
  echo "--rpc-url $RPC_URL --network-passphrase $NETWORK_PASSPHRASE --source-account $ACCOUNT"
}

# Run a soroban invocation and print the tx hash alongside the result.
invoke() {
  local hash
  hash=$(soroban contract invoke $(soroban_args) "$@" --json 2>&1) || {
    echo "$hash"
    return 1
  }
  # Best-effort tx hash extraction from the JSON tx meta.
  local tx_hash
  tx_hash=$(echo "$hash" | grep -oE '"hash":"[A-Za-z0-9]+' | head -1 | cut -d'"' -f4)
  [ -n "$tx_hash" ] && echo "tx=$tx_hash"
  echo "$hash"
  return 0
}

echo "=== VeriTix Contract Integration Test (localnet) ==="
echo ""

# --- Step 1: Build WASM ---
echo "--- Step 1: Build WASM ---"
if make -C "$ROOT/veritixpay/contract/token" build >/dev/null 2>&1; then
  pass "make build produced WASM"
else
  fail "make build failed; WASM not produced"
fi

WASM=$(ls "$ROOT"/target/wasm32v1-none/release/*.wasm 2>/dev/null | head -1)
[ -z "$WASM" ] && fail "No WASM found at target/wasm32v1-none/release/" || pass "WASM found: $WASM"

# --- Prepare a funded source account (valid addresses instead of hardcoded fakes) ---
if ! soroban keys address "$ACCOUNT" >/dev/null 2>&1; then
  soroban keys generate "$ACCOUNT" --rpc-url "$RPC_URL" \
    --network-passphrase "$NETWORK_PASSPHRASE" --fund >/dev/null
fi
ADMIN=$(soroban keys address "$ACCOUNT")
SECOND=$(soroban keys generate "${ACCOUNT}-2" --rpc-url "$RPC_URL" \
  --network-passphrase "$NETWORK_PASSPHRASE" --fund --overwrite >/dev/null 2>&1 && soroban keys address "${ACCOUNT}-2")

# --- Step 2: Deploy ---
echo "--- Step 2: Deploy contract ---"
CONTRACT_ID=$(soroban contract deploy $(soroban_args) --wasm "$WASM")
if [ -n "$CONTRACT_ID" ]; then
  pass "Contract deployed: $CONTRACT_ID"
else
  fail "Contract deployment failed"
fi

# --- Step 3: Initialize ---
echo "--- Step 3: Initialize contract ---"
if invoke --id "$CONTRACT_ID" -- initialize --admin "$ADMIN" \
    --name VeriTix --symbol VTX --decimal 7; then
  pass "Contract initialized"
else
  fail "Initialization failed"
fi

# --- Step 4: Mint 1000 ---
echo "--- Step 4: Mint 1000 tokens ---"
if invoke --id "$CONTRACT_ID" -- mint --admin "$ADMIN" --to "$ADMIN" --amount 1000; then
  pass "Minted 1000 tokens"
else
  fail "Mint failed"
fi

# --- Step 5: Balance = 1000 ---
echo "--- Step 5: Check balance ---"
BALANCE=$(soroban contract invoke $(soroban_args) --id "$CONTRACT_ID" -- balance --id "$ADMIN")
if [ "$BALANCE" = "1000" ]; then
  pass "Balance is 1000"
else
  fail "Expected balance 1000, got: $BALANCE"
fi

# --- Step 6: Transfer 100 ---
echo "--- Step 6: Transfer 100 tokens ---"
if invoke --id "$CONTRACT_ID" -- transfer_with_memo \
    --from "$ADMIN" --to "$SECOND" --amount 100 --memo "00"; then
  pass "Transferred 100 tokens"
else
  fail "Transfer failed"
fi

# --- Step 7: Create escrow ---
echo "--- Step 7: Create escrow ---"
# Top up the depositor so the escrow amount clears MIN_ESCROW_AMOUNT.
if invoke --id "$CONTRACT_ID" -- mint --admin "$ADMIN" --to "$ADMIN" \
    --amount "$MIN_ESCROW_AMOUNT"; then
  EXPIRY=$(( $(soroban ledger sequence --rpc-url "$RPC_URL" \
      --network-passphrase "$NETWORK_PASSPHRASE" 2>/dev/null || echo 1000) + 1000 ))
  ESCROW_ID=$(soroban contract invoke $(soroban_args) --id "$CONTRACT_ID" -- create_escrow \
      --depositor "$ADMIN" --beneficiary "$SECOND" --token "$CONTRACT_ID" \
      --amount "$MIN_ESCROW_AMOUNT" --expiry_ledger "$EXPIRY" --memo "00")
  if [ -n "$ESCROW_ID" ] && [ "$ESCROW_ID" != "0" ]; then
    pass "Escrow created with id: $ESCROW_ID"
  else
    fail "Escrow creation returned no id"
  fi
else
  fail "Escrow funding mint failed"
fi

# --- Summary ---
echo ""
echo "=== Results ==="
echo "PASS: $PASS"
echo "FAIL: $FAIL"

if [ "$FAIL" -gt 0 ]; then
  exit 1
fi
