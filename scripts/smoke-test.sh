#!/usr/bin/env bash
#
# Smoke-test for the hyperswitch-card-vault data / v2-vault / fingerprint flows.
#
# The API surface is identical whether the server uses the INTERNAL key manager
# (master key, no external service) or the EXTERNAL key manager (encryption
# service) — only the way you *run the server* differs. So this one script
# validates both. See docs/testing-flows.md for how to start the server each way.
#
# Requires: curl, jq. Server reachable at $BASE with tenant $TENANT.
#
#   BASE=http://localhost:3001 TENANT=public ./scripts/smoke-test.sh
#   RUN_EXPIRY=1 ./scripts/smoke-test.sh      # also runs the (slow) TTL-expiry case
#
set -uo pipefail

BASE="${BASE:-http://localhost:3001}"
TENANT="${TENANT:-public}"
RUN="${RUN:-$(date +%s)}"          # unique suffix → reruns start fresh (dedup-independent)
MID="merchant_$RUN"
CID="customer_$RUN"
CARD='{"card_number":"4111111111111111","name_on_card":"John Doe","card_exp_month":"03","card_exp_year":"2030","card_brand":"VISA","card_isin":"411111","nick_name":"v"}'

command -v jq >/dev/null || { echo "jq is required"; exit 2; }

pass=0; fail=0
g=$'\e[32m'; r=$'\e[31m'; d=$'\e[2m'; x=$'\e[0m'
code=""; body=""

# req METHOD PATH [JSON] [EXTRA_HEADER]  -> sets $code, $body
req() {
  local method=$1 path=$2 data=${3:-} extra=${4:-}
  local args=(-s -o /tmp/st_body -w '%{http_code}' -X "$method"
              -H "content-type: application/json")
  [[ "${5:-with_tenant}" != "no_tenant" ]] && args+=(-H "x-tenant-id: $TENANT")
  [[ -n $extra ]] && args+=(-H "$extra")
  [[ -n $data  ]] && args+=(-d "$data")
  code=$(curl "${args[@]}" "$BASE$path")
  body=$(cat /tmp/st_body)
}
jqv()  { echo "$body" | jq -r "$1" 2>/dev/null; }
ok()   { pass=$((pass+1)); printf "  ${g}✓${x} %s\n" "$1"; }
bad()  { fail=$((fail+1)); printf "  ${r}✗ %s${x}\n     ${d}code=%s body=%s${x}\n" "$1" "$code" "$body"; }
want_code() { [[ "$code" == "$1" ]] && ok "$2"   || bad "$2 (wanted HTTP $1)"; }
want()      { [[ "$1" == "$2"   ]] && ok "$3"   || bad "$3 (wanted '$2', got '$1')"; }

echo "== $BASE  tenant=$TENANT  run=$RUN =="

echo "[health]"
req GET /health
want_code 200 "GET /health"

echo "[add card]"
req POST /data/add "{\"merchant_id\":\"$MID\",\"merchant_customer_id\":\"$CID\",\"card\":$CARD,\"ttl\":3600}"
want_code 200 "TC1.1 add new card"
REF=$(jqv '.payload.card_reference')
[[ -n $REF && $REF != null ]] && ok "card_reference=$REF" || bad "no card_reference"
want "$(jqv '.payload.duplication_check')" "null" "TC1.1 duplication_check=null (new card)"

req POST /data/add "{\"merchant_id\":\"$MID\",\"merchant_customer_id\":\"$CID\",\"card\":$CARD,\"ttl\":3600}"
want "$(jqv '.payload.duplication_check')" "duplicated" "TC1.2 re-add same card → duplicated  [dedup path]"
want "$(jqv '.payload.card_reference')" "$REF" "TC1.2 same card_reference returned  [insert_or_get]"

req POST /data/add "{\"merchant_id\":\"$MID\",\"merchant_customer_id\":\"$CID\",\"card\":{\"card_number\":\"4111111111111111\",\"name_on_card\":\"Jane NEW\",\"card_exp_month\":\"05\",\"card_exp_year\":\"2031\"},\"ttl\":3600}"
want "$(jqv '.payload.duplication_check')" "meta_data_changed" "TC1.3 same number, new metadata → meta_data_changed"

req POST /data/add "{\"merchant_id\":\"$MID\",\"merchant_customer_id\":\"$CID\",\"card\":{\"card_number\":\"1234567890123456\"},\"ttl\":3600}"
want_code 400 "TC1.7 invalid card number → 400"

req POST /data/add "{\"merchant_id\":\"$MID\",\"merchant_customer_id\":\"$CID\",\"card\":$CARD,\"ttl\":0}"
want_code 400 "TC1.8 past TTL → 400"

req POST /data/add "{\"merchant_id\":\"$MID\",\"merchant_customer_id\":\"$CID\",\"card\":$CARD,\"ttl\":3600}" "" no_tenant
want_code 400 "TC1.9 missing x-tenant-id → 400"

echo "[retrieve card]"
req POST /data/retrieve "{\"merchant_id\":\"$MID\",\"merchant_customer_id\":\"$CID\",\"card_reference\":\"$REF\"}"
want_code 200 "TC2.1 retrieve existing"
want "$(jqv '.payload.card.card_number')" "4111111111111111" "TC2.1 card_number round-trips"

req POST /data/retrieve "{\"merchant_id\":\"$MID\",\"merchant_customer_id\":\"$CID\",\"card_reference\":\"missing-$RUN\"}"
want_code 404 "TC2.2 retrieve missing → 404  [find_by_* erroring]"

req POST /data/retrieve "{\"merchant_id\":\"$MID\",\"merchant_customer_id\":\"WRONG\",\"card_reference\":\"$REF\"}"
want_code 404 "TC2.3 wrong customer → 404"

echo "[fingerprint]"
req POST /data/fingerprint "{\"data\":\"4111111111111111\",\"key\":\"k-$RUN\"}"
want_code 200 "TC4.1 fingerprint new"
FP=$(jqv '.fingerprint_id'); [[ -n $FP && $FP != null ]] && ok "fingerprint_id=$FP" || bad "no fingerprint_id"
req POST /data/fingerprint "{\"data\":\"4111111111111111\",\"key\":\"k-$RUN\"}"
want "$(jqv '.fingerprint_id')" "$FP" "TC4.2 same data+key → same id  [dedup]"
req POST /data/fingerprint "{\"data\":\"4111111111111111\",\"key\":\"k2-$RUN\"}"
[[ "$(jqv '.fingerprint_id')" != "$FP" ]] && ok "TC4.3 different key → different id" || bad "TC4.3 id should differ"
req POST /data/fingerprint '{"data":"d","key":"k"}' "x-fingerprint-id: short"
want_code 400 "TC4.6 bad x-fingerprint-id (not 20 alnum) → 400"

echo "[v2 vault]"
EID="entity_$RUN"; VID="vault_$RUN"
req POST "/api/v2/vault/add" "{\"entity_id\":\"$EID\",\"vault_id\":\"$VID\",\"data\":{\"acct\":\"123\"},\"ttl\":3600}"
want_code 200 "TC5.1 vault insert"
req POST "/api/v2/vault/add" "{\"entity_id\":\"$EID\",\"vault_id\":\"$VID\",\"data\":{\"acct\":\"NOPE\"},\"ttl\":3600}"
want_code 200 "TC5.2 insert same again (insert_or_get)"
req POST /api/v2/vault/retrieve "{\"entity_id\":\"$EID\",\"vault_id\":\"$VID\"}"
want "$(jqv '.data.acct')" "123" "TC5.2 insert kept original (no overwrite)"
req POST "/api/v2/vault/add?mode=upsert" "{\"entity_id\":\"$EID\",\"vault_id\":\"$VID\",\"data\":{\"acct\":\"OVERWRITTEN\"},\"ttl\":7200}"
want_code 200 "TC5.3 vault upsert"
req POST /api/v2/vault/retrieve "{\"entity_id\":\"$EID\",\"vault_id\":\"$VID\"}"
want "$(jqv '.data.acct')" "OVERWRITTEN" "TC5.3 upsert overwrote data"
req POST /api/v2/vault/retrieve "{\"entity_id\":\"$EID\",\"vault_id\":\"missing-$RUN\"}"
want_code 404 "TC5.5 vault retrieve missing → 404  [find_by_* erroring]"
req POST /api/v2/vault/delete "{\"entity_id\":\"$EID\",\"vault_id\":\"$VID\"}"
want_code 200 "TC5.6 vault delete existing"
req POST /api/v2/vault/delete "{\"entity_id\":\"$EID\",\"vault_id\":\"missing-$RUN\"}"
want_code 200 "TC5.7 vault delete missing → 200 (idempotent)"

echo "[entity provisioning]"
# Explicit, idempotent provisioning endpoint. Backing table is config-driven:
# `merchant` (internal key manager) or `entity` (external key manager).
PENT="prov_$RUN"
req POST /entity "{\"entity_id\":\"$PENT\"}"
want_code 200 "TC6.1 POST /entity create new"
want "$(jqv '.entity_id')" "$PENT" "TC6.1 entity_id echoed back"
CA1=$(jqv '.created_at|tostring'); [[ -n $CA1 && $CA1 != null ]] && ok "TC6.1 created_at present ($CA1)" || bad "TC6.1 no created_at"

req POST /entity "{\"entity_id\":\"$PENT\"}"
want_code 200 "TC6.2 POST /entity again → 200 (idempotent)"
want "$(jqv '.created_at|tostring')" "$CA1" "TC6.2 same created_at → no new record  [find-or-create]"

req POST /entity '{"entity_id":"  "}'
want_code 400 "TC6.3 blank entity_id → 400  [validation]"

req POST /entity '{}'
want_code 422 "TC6.4 missing entity_id field → 422  [deserialization]"

req POST /entity '{"entity_id":"x"}' "" no_tenant
want_code 400 "TC6.5 missing x-tenant-id → 400"

# Full lifecycle on one pre-provisioned entity: every downstream add finds the existing
# record and must NOT take the deprecated lazy auto-create path. Both the v1 card flow
# (merchant_id) and the v2 vault flow (entity_id) key off the same provisioned id.
E2E="prov_e2e_$RUN"
req POST /entity "{\"entity_id\":\"$E2E\"}"
want_code 200 "TC6.6 provision entity up-front"
req POST /data/add "{\"merchant_id\":\"$E2E\",\"merchant_customer_id\":\"$CID\",\"card\":$CARD,\"ttl\":3600}"
want_code 200 "TC6.6 add card on pre-provisioned entity"
E2EREF=$(jqv '.payload.card_reference')
req POST /data/retrieve "{\"merchant_id\":\"$E2E\",\"merchant_customer_id\":\"$CID\",\"card_reference\":\"$E2EREF\"}"
want "$(jqv '.payload.card.card_number')" "4111111111111111" "TC6.6 retrieve card round-trips"
req POST /api/v2/vault/add "{\"entity_id\":\"$E2E\",\"vault_id\":\"vault_$RUN\",\"data\":{\"acct\":\"123\"},\"ttl\":3600}"
want_code 200 "TC6.6 vault add on same entity"
req POST /api/v2/vault/retrieve "{\"entity_id\":\"$E2E\",\"vault_id\":\"vault_$RUN\"}"
want "$(jqv '.data.acct')" "123" "TC6.6 vault retrieve round-trips"
req POST /data/delete "{\"merchant_id\":\"$E2E\",\"merchant_customer_id\":\"$CID\",\"card_reference\":\"$E2EREF\"}"
want_code 200 "TC6.6 delete card"
req POST /data/retrieve "{\"merchant_id\":\"$E2E\",\"merchant_customer_id\":\"$CID\",\"card_reference\":\"$E2EREF\"}"
want_code 404 "TC6.6 retrieve after delete → 404"
# Optional: with LOG=<server log file>, assert the migration signal stayed silent for the
# pre-provisioned entity (no auto-create happened at any step).
if [[ -n "${LOG:-}" && -f "${LOG:-}" ]]; then
  if grep "add_flow_auto_create" "$LOG" | grep -q "$E2E"; then
    bad "TC6.6 unexpected add_flow_auto_create warn for pre-provisioned $E2E"
  else
    ok "TC6.6 no auto-create warn for pre-provisioned entity  [migration signal silent]"
  fi
fi

echo "[delete card]"
req POST /data/delete "{\"merchant_id\":\"$MID\",\"merchant_customer_id\":\"$CID\",\"card_reference\":\"$REF\"}"
want_code 200 "TC3.1 delete card"
req POST /data/retrieve "{\"merchant_id\":\"$MID\",\"merchant_customer_id\":\"$CID\",\"card_reference\":\"$REF\"}"
want_code 404 "TC3.1 retrieve after delete → 404"
req POST /data/delete "{\"merchant_id\":\"$MID\",\"merchant_customer_id\":\"$CID\",\"card_reference\":\"$REF\"}"
want_code 200 "TC3.2 delete missing → 200 (idempotent)"

if [[ "${RUN_EXPIRY:-0}" == "1" ]]; then
  echo "[ttl expiry]"
  req POST /data/add "{\"merchant_id\":\"$MID\",\"merchant_customer_id\":\"exp-$RUN\",\"card\":$CARD,\"ttl\":1}"
  EREF=$(jqv '.payload.card_reference')
  sleep 2
  req POST /data/retrieve "{\"merchant_id\":\"$MID\",\"merchant_customer_id\":\"exp-$RUN\",\"card_reference\":\"$EREF\"}"
  want_code 404 "TC2.4 retrieve expired card → 404 (+ async delete)"
fi

echo
printf "== ${g}%d passed${x}, ${r}%d failed${x} ==\n" "$pass" "$fail"
[[ $fail == 0 ]]
