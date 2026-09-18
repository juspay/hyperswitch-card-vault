# Database Upgrade Using KV

This runbook describes how to upgrade the database by promoting a larger read
replica while KV reduces the impact of the failover.

This runbook assumes that two Locker pods are running in production behind an
Application Load Balancer (ALB).

## Important Behavior

- The supported KV transition order is `disabled -> enabled -> soft_kill -> disabled`.
- `enabled -> disabled` and `soft_kill -> enabled` are not supported.
- `use_replica` sends reads to the configured replica. Database writes still use
  the primary.
- KV does not remove all database dependency. New inserts and Redis misses may
  still query PostgreSQL during failover.
- Runtime config is refreshed independently by each Locker pod. Verify the state
  applied by both pods instead of waiting for a fixed number of seconds.
- KV entries have a configured Redis TTL. The default is 900 seconds, but the
  production value must be checked before the activity.
- Keep both pods running during KV state transitions. In particular, do not
  restart a pod during `soft_kill` because a new process starts in `disabled` and
  cannot transition directly to `soft_kill`.

## Mandatory Entity Prerequisites

Entity creation during the card-create flow cannot be relied on while this
activity is in progress. Complete these steps before starting the database or KV
changes.

### 1. Enable Entity Creation During Merchant Creation

Set the following runtime environment flag to `true` in Router:

```text
create_entity_on_merchant_create=true
```

Deploy or restart Router as required for the environment change to take effect.
Verify the effective value and confirm that creating a test merchant account also
creates its entity record in Card Vault.

Keep this flag enabled throughout the database upgrade activity.

### 2. Backfill Existing Merchants

After the Router flag is active, backfill entity records for every existing
production `merchant_id` that does not have a corresponding entity record in
Card Vault.

Before proceeding with the upgrade, verify:

- The backfill is idempotent or safe to retry.
- All failed records have been retried or resolved.
- No duplicate entity records were created.
- A final reconciliation reports zero production merchants without a Card Vault
  entity record.
- Merchants created while the backfill was running also have entity records.

## Pre-Upgrade Checks

- Take a database snapshot and confirm point-in-time recovery is available.
- Create the larger read replica and wait for replication lag to become acceptable.
- Ensure two healthy read replicas are available before failover so that at least
  one replica remains after the larger replica is promoted.
- Confirm the primary, replica, Redis, and runtime-config health checks pass.
- Confirm Redis has enough memory and is not evicting keys.
- Confirm the drainer is healthy and its replay lag is lower than the Redis KV TTL.
- Confirm how the drainer reconnects and retries database writes during failover.
- Confirm the application and drainer database endpoints will follow the promoted
  writer.
- Confirm one Locker pod can safely handle the full production traffic during a
  KV state transition.
- Identify both Locker targets in the ALB target group and record the current
  target-group configuration.
- Confirm both Locker targets are healthy and that operators have permission to
  deregister and re-register a target.
- Confirm the ALB deregistration delay and allow enough time for in-flight
  requests to drain.
- Define rollback and alert thresholds before starting.
- Avoid deployments, schema migrations, and unrelated infrastructure changes
  during the activity.

## Prerequisite: Runtime Config Key Rename

The runtime config row key changes from `locker_runtime_config` to `kv_config`. The
rename must be applied to **every tenant schema** **before** any pod running the new
binary starts (handled internally, outside this repo).

Order matters. If a new pod boots first, it seeds a fresh `kv_config` row from the static
config before the rename runs, and the rename then finds `kv_config` already present and
skips it — leaving the row holding the live state under the old key, silently reset to
the seeded value once it is dropped. Apply the rename to all tenant schemas, confirm it,
and only then deploy:

```bash
# per tenant schema
psql "$DATABASE_URL" -c "SET search_path TO ${TENANT_SCHEMA}; SELECT key FROM configs;"
# expect: kv_config   (and no locker_runtime_config)
```

## Updating Runtime Config

Use the following request to write `kv_config` for a given tenant.
Provide the target tenant ID and the shared `admin_api_key` from the deployed
`[runtime_config]` config (`admin_api_key` in TOML / `LOCKER__RUNTIME_CONFIG__ADMIN_API_KEY`).
The `key` must name a runtime config the deployed binary knows about — `kv_config` here —
and `value` must match that config's struct. Both are fixed at compile time, so an unknown
key or an unrecognised field inside `value` is rejected before anything is written:

```bash
curl --location "https://${LOCKER_HOST}/runtime-config" \
  --header "x-tenant-id: ${TENANT_ID}" \
  --header "x-internal-api-key: ${LOCKER_RUNTIME_CONFIG_ADMIN_API_KEY}" \
  --header 'Content-Type: application/json' \
  --data '{"key":"kv_config","value":{"use_replica":false,"enable_kv":"disabled"}}'
```

After the update, verify the applied state directly on both pods through
`/health/runtime-config` before continuing (e.g., `curl -H "x-tenant-id: $TENANT_ID"
https://${LOCKER_HOST}/health/runtime-config`). Do not use the ALB endpoint
for verification because repeated requests may reach the same pod. The endpoint reports one
entry per enabled runtime config under `runtime_config`, keyed by its config key —
`runtime_config.kv_config.status` is `available` once the row reads
cleanly, with the applied value under `runtime_config.kv_config.config`;
the effective routing state is mirrored under `storage` (`use_replica`, `kv_state`).

## ALB Routing for KV State Changes

Use this procedure before every change to `enable_kv`:

1. Choose one Locker pod as the active pod.
2. Deregister the other pod from the ALB target group.
3. Wait for the removed target to finish draining existing connections. Confirm
   that all new Locker traffic is reaching only the active pod.
4. Keep the removed pod running so that it continues serving traffic with config reads hitting Redis/Postgres directly.
5. Apply the required `enable_kv` configuration from the relevant step below.
6. Query `/health/runtime-config` directly on both pods and wait until they report
   the same expected KV state.
7. If either pod reports a different state, keep the out-of-sync pod out of the
   ALB and do not continue the activity.
8. Re-register the removed pod with the ALB only after both pods are in sync.
9. Wait until both ALB targets are healthy, then confirm traffic is distributed
   across both pods and API errors remain within the acceptable limit.

ALB traffic may not be exactly split 50/50 at every moment because of persistent
connections, but both healthy targets should receive traffic over time.

## Upgrade Steps

### 1. Confirm the Initial State

```json
{ "use_replica":false,"enable_kv":"disabled" }
```

Verify on both Locker pods:

- `use_replica` is `false`.
- KV state is `disabled`.
- Runtime config status is available.
- PostgreSQL and Redis are healthy.

### 2. Enable KV

Follow the ALB routing procedure above so that only one pod receives traffic
while KV changes from `disabled` to `enabled`.

```json
{ "use_replica":false,"enable_kv":"enabled" }
```

Before continuing, verify:

- Both pods report KV state as `enabled`.
- Redis remains healthy and has no evictions.
- KV and drainer operations are succeeding.
- Drainer lag remains within the acceptable limit.
- Basic add, retrieve, update, and delete requests succeed.
- Both pods are healthy in the ALB and receiving traffic after balanced routing
  is restored.

### 3. Enable Replica Reads

```json
{ "use_replica":true,"enable_kv":"enabled" }
```

Before continuing, verify:

- Both pods report `use_replica` as `true`.
- The replica health check passes.
- Replication lag, read errors, and latency remain acceptable.

### 4. Perform the Database Failover

Promote the larger read replica while KV and replica reads remain enabled.

During failover, monitor:

- API errors and latency.
- Database connections and writer availability.
- Redis health, memory, and errors.
- Drainer failures, pending entries, and replay lag.
- Replica health and replication lag.

With KV and replica reads enabled, Locker's synchronous PostgreSQL operations
are reads routed to the configured read replica. If that replica remains
reachable and sufficiently caught up, the writer failover should not interrupt
card operations. KV writes will accumulate in the drainer until the new writer
becomes available.

The existence of another healthy replica is not sufficient by itself. Locker's
configured replica endpoint must route to that healthy replica. Replica
unavailability, replication lag, connection resets, or DNS changes can still
cause read failures, and the drainer backlog must remain within the Redis KV TTL.

### 5. Validate the New Primary

Before continuing, verify:

- The writer endpoint points to the new larger instance.
- Database reads, writes, and deletes succeed.
- Add, retrieve, update, and delete API tests succeed.
- The drainer can write to the new primary and has caught up.
- The remaining read replica is healthy and caught up.

### 6. Disable Replica Reads

```json
{ "use_replica":false,"enable_kv":"enabled" }
```

Verify both pods report `use_replica` as `false` and reads against the new primary
succeed.

The remaining read replicas can now be upgraded one at a time. Wait for each
replica to become healthy and catch up before upgrading the next one.

### 7. Put KV Into Soft Kill

Only start this step after the new primary and upgraded replicas are stable.
Follow the ALB routing procedure above so that only one pod receives traffic
while KV changes from `enabled` to `soft_kill`.

```json
{ "use_replica":false,"enable_kv":"soft_kill" }
```

Verify both pods report KV state as `soft_kill`, then restore balanced traffic
only after both ALB targets are healthy.

During soft kill, new or Redis-absent entities use PostgreSQL. Existing Redis
entities continue using KV until they expire. Updates to an existing KV entity
can refresh its TTL, so do not rely only on elapsed time.

### 8. Wait for KV to Drain

KV can be disabled when all of the following are true:

- All KV entity keys, lookup keys, and tombstones have expired naturally.
- No new KV keys appear during a grace period that covers in-flight requests.
- A second Redis check confirms the keys remain absent.
- Drainer consumer lag is zero.
- Drainer pending entries are zero.
- All queued changes have reached PostgreSQL.
- No unresolved drainer-push or replay failures exist.

Do not manually delete or flush Redis keys to make this step complete faster.
If live traffic keeps refreshing KV keys, quiesce traffic for the required TTL
window or remain in soft kill.

### 9. Disable KV

Follow the ALB routing procedure above so that only one pod receives traffic
while KV changes from `soft_kill` to `disabled`.

```json
{ "use_replica":false,"enable_kv":"disabled" }
```

Verify both Locker pods report:

- `use_replica` is `false`.
- KV state is `disabled`.
- Runtime config status is available.
- Both ALB targets are healthy and receiving traffic after balanced routing is
  restored.

Continue monitoring PostgreSQL load, database connections, API errors, and data
correctness after the activity.

## Rollback Notes

- If replica reads cause issues, set `use_replica` to `false` immediately.
- If failover fails, keep KV `enabled` while restoring database availability.
- Do not move to `soft_kill` until the new primary is fully stable.
- A direct `enabled -> disabled` transition is rejected.
- A direct `soft_kill -> enabled` transition is rejected.
- Do not disable KV while drainer entries are pending or PostgreSQL is missing
  replayed changes.
- If the pods report different KV states, keep the out-of-sync pod deregistered
  and do not apply the next runtime-config change.
- If the active pod fails while the other pod is deregistered, verify the
  deregistered pod has the expected KV state before routing traffic to it.
- Do not restart either pod during `soft_kill`. Keep a restarted pod out of the
  ALB until the cluster returns safely to `disabled`.

## Useful Health Endpoints

- `/health/runtime-config`: confirms the runtime config applied by a specific
  Locker pod. Query both pods directly.
- `/health/diagnostics`: confirms primary database, replica, and Redis health for
  a specific Locker pod. Query both pods directly.

## Observability Links

Replace these placeholders with production links before the activity:

- [Locker API metrics dashboard](ADD_LOCKER_API_METRICS_DASHBOARD_URL)
- [KV and Redis metrics dashboard](ADD_KV_REDIS_DASHBOARD_URL)
- [Primary database metrics dashboard](ADD_PRIMARY_DATABASE_DASHBOARD_URL)
- [Read replica and replication lag dashboard](ADD_READ_REPLICA_DASHBOARD_URL)
- [Drainer metrics and backlog dashboard](ADD_DRAINER_DASHBOARD_URL)
- [ALB target health dashboard](ADD_ALB_TARGET_HEALTH_DASHBOARD_URL)
- [ALB per-target request distribution](ADD_ALB_REQUEST_DISTRIBUTION_URL)
- [Locker logs](ADD_LOCKER_LOGS_URL)
- [Drainer logs](ADD_DRAINER_LOGS_URL)
- [Database logs](ADD_DATABASE_LOGS_URL)
- [Production alerts](ADD_PRODUCTION_ALERTS_URL)
