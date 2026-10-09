#!/usr/bin/env bash
# Burn TARI from a wallet daemon account's public balance into the official Caravel Burn Wallet.
#
#   export TARI_WALLET_DAEMON_API_KEY=…         # never put it on the command line
#   ./burn.sh 1000000                           # dry run only: prints the exact fee
#   ./burn.sh 1000000 --submit                  # dry run, then submit at exactly that fee
#
# Amount in µTARI (1 TARI = 1,000,000). The fee is paid by the daemon's DEFAULT account, from its
# public balance, on top of the amount. Needs curl and jq.
set -euo pipefail

AMOUNT=${1:?amount in µTARI}
SUBMIT=${2:-}
WALLET=${WALLET:-http://localhost:5100/json_rpc}
BURN_WALLET=component_2e91fb78b73440dd114bdbb887d256d760fa3be3276d7b833f4ab1b23d796029
TARI=resource_0101010101010101010101010101010101010101010101010101010101010101
: "${TARI_WALLET_DAEMON_API_KEY:?set TARI_WALLET_DAEMON_API_KEY}"

rpc() {
  local out
  out=$(curl -sf "$WALLET" -H 'content-type: application/json' \
    -H "authorization: Bearer $TARI_WALLET_DAEMON_API_KEY" \
    -d "$(jq -n --arg m "$1" --argjson p "$2" '{jsonrpc:"2.0", id:1, method:$m, params:$p}')")
  if jq -e .error <<<"$out" >/dev/null; then echo "$1: $(jq -c .error <<<"$out")" >&2; exit 1; fi
  jq .result <<<"$out"
}

ACCOUNT=$(rpc accounts.get_default '{}' | jq -r .account.component_address)

MANIFEST='
fn main() {
    let mut account = global!["account"];
    let burn_wallet = global!["burn_wallet"];
    let bucket = account.withdraw(global!["tari"], global!["amount"]);
    burn_wallet.deposit(bucket);
}'

submit() { # max_fee dry_run
  rpc transactions.submit_manifest "$(jq -n --arg m "$MANIFEST" --arg a "$ACCOUNT" --arg b "$BURN_WALLET" \
    --arg t "$TARI" --arg n "$AMOUNT" --argjson f "$1" --argjson d "$2" \
    '{manifest:$m, variables:{account:$a, burn_wallet:$b, tari:$t, amount:$n}, max_fee:$f, dry_run:$d}')"
}

# 1. Dry run (nothing is spent). It must Accept; its required fee is the exact fee.
DRY=$(submit 100000 true)
jq -e '.result.finalize.result | has("Accept")' <<<"$DRY" >/dev/null \
  || { echo "dry run did not Accept: $(jq -c .result.finalize.result <<<"$DRY" | head -c 400)" >&2; exit 1; }
FEE=$(jq -r .required_fees <<<"$DRY")
DEPOSITED=$(jq -r --arg b "$BURN_WALLET" '[.result.finalize.events[] | select(.substate_id == $b and .topic == "CaravelBurnWallet.Deposit") | .payload.amount] | join(",")' <<<"$DRY")
echo "dry run   Accept: burns $AMOUNT µTARI from $ACCOUNT, fee $FEE µTARI; Deposit event from the burn wallet: amount $DEPOSITED"

# 2. The same manifest at exactly that fee, dry-run again, and then submitted.
EXACT=$(submit "$FEE" true)
jq -e '.result.finalize.result | has("Accept")' <<<"$EXACT" >/dev/null || { echo "rejected at the exact fee" >&2; exit 1; }
echo "dry run   Accept at max_fee $FEE"
[ "$SUBMIT" = --submit ] || exit 0

TX=$(submit "$FEE" false | jq -r .transaction_id)
echo "submitted $TX"
RESULT=$(rpc transactions.wait_result "$(jq -n --arg t "$TX" '{transaction_id:$t, timeout_secs:300}')")
jq -e '.result.result | has("Accept")' <<<"$RESULT" >/dev/null || { echo "did not Accept: $(jq -c .result.result <<<"$RESULT" | head -c 400)" >&2; exit 1; }
echo "ACCEPT    fee $(jq -r .final_fee <<<"$RESULT") µTARI"
