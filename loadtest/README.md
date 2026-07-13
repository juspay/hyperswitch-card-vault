# Hyperswitch Card Vault - Production Traffic Load Test

This directory contains a [k6](https://k6.io/) based load-test suite that simulates realistic production traffic against the Hyperswitch Card Vault service.

## Files

- `scripts/production-traffic.js` - main k6 script covering health, legacy `/data`, vault v2, fingerprint, negative, and optional custodian scenarios.
- `scripts/http-rs.js` - older minimal script posting one encrypted payload.
- `scripts/settings.js` - shared k6 options.
- `Makefile` - convenience targets for docker and local runs.
- `docker-compose.yaml` - locker, postgres, grafana, influxdb and k6 setup.
- `config/` - grafana dashboards and datasource provisioning.

## What the script covers

The production-traffic script exercises every curl present in `docs/collection/hyperswitch-card-vault.postman_collection.json` as weighted production scenarios rather than a linear walk-through:

- **Health** - `GET /health`, `GET /health/diagnostics`
- **Legacy Data API** - `/data/add`, `/data/retrieve`, `/data/delete`
  - happy-path add → retrieve → delete → retrieve-miss
  - duplicate add with same metadata
  - metadata-changed duplicate add
  - delayed retrieve after a configured wait
- **Vault API v2** - `/api/v2/vault/add?mode=upsert`, `/retrieve`, `/delete`
  - happy-path add → retrieve → delete → retrieve-miss
  - upsert overwrite scenario
- **Fingerprint API** - `/data/fingerprint`
  - create fingerprint
  - reuse existing fingerprint
  - caller-supplied `x-fingerprint-id`
- **Negative scenarios**
  - invalid card number
  - missing `x-tenant-id` header
  - expired TTL (retrieve returns 404)
  - retrieve non-existent card reference
- **Key Custodian (optional)** - `/custodian/key1`, `/custodian/key2`, `/custodian/decrypt`

## Configuration

All behaviour is controlled through environment variables. The defaults are chosen so the same script works against the docker loadtest target and a local HTTPS server.

| Variable | Default | Description |
|---|---|---|
| `BASE_URL` | `https://127.0.0.1:3001` (Makefile) / `http://locker_server:8080` (docker-compose) | Locker base URL. |
| `TENANT_ID` | `public` (Makefile) / `hyperswitch` (docker-compose) | Tenant passed in `x-tenant-id`. |
| `DURATION` | `5m` | Test duration. Accepts k6 duration strings (`30s`, `5m`, `2h`). |
| `VUS` | `5` | Number of concurrent virtual users. |
| `RUN_FOREVER` | `false` | Set to `true` to run until interrupted. Internally sets duration to a very large value. |
| `INSECURE_SKIP_TLS_VERIFY` | `true` | Skip TLS verification for local self-signed certificates. |
| `ENABLE_NEGATIVE` | `true` | Enable negative-scenario traffic. |
| `ENABLE_CUSTODIAN` | `false` | Enable custodian unlock traffic. Requires `KEY1` and `KEY2`. |
| `KEY1` | `""` | First custodian key (hex string). |
| `KEY2` | `""` | Second custodian key (hex string). |
| `DELAYED_RETRIEVE_ENABLED` | `true` | Enable the delayed-retrieve scenario. |
| `DELAYED_RETRIEVE_DELAY` | `30s` | How long to wait between add and retrieve. |
| `DELAYED_RETRIEVE_TTL` | `3600` | TTL used for the delayed-retrieve card. Must be larger than the delay. |
| `WEIGHT_*` | varies | Relative weights for each scenario. See the script header for defaults. |

The default weight mix is:

```text
health=5, legacy_flow=25, legacy_duplicate=10, legacy_metadata_changed=10,
delayed_retrieve=10, v2_flow=20, v2_update=10, fingerprint_create=10,
fingerprint_reuse=10, fingerprint_supplied_id=5, negative=5, custodian=2 (when enabled)
```

## Running locally

### 1. Start the locker service

Follow `docs/guides/local-development.md` to create certificates, the database, and run migrations, then start the server:

```bash
cargo run --bin locker
```

The local config serves HTTPS on `https://127.0.0.1:3001` with tenant `public`.

### 2. Install k6

See <https://grafana.com/docs/k6/latest/set-up/install-k6/>.

### 3. Run the script

```bash
cd loadtest
make local-test
```

Or run k6 directly:

```bash
cd loadtest
mkdir -p results
k6 run --console-output=results/unexpected-failures.jsonl scripts/production-traffic.js
```

Override any variable:

```bash
DURATION=2m VUS=10 BASE_URL=https://127.0.0.1:3001 TENANT_ID=public \
  k6 run --console-output=results/unexpected-failures.jsonl scripts/production-traffic.js
```

To run until interrupted:

```bash
RUN_FOREVER=true k6 run --console-output=results/unexpected-failures.jsonl scripts/production-traffic.js
```

### 4. Results

- `loadtest/results/unexpected-failures.jsonl` - JSON Lines of every unexpected failure, including scenario, URL, method, status, expected statuses, request body, and response body.
- `loadtest/results/summary.json` - Aggregated test summary written by `handleSummary`.

The `loadtest/results/` directory is gitignored; artifacts are meant for local inspection only. Both `make local-test` and `make test` truncate the failure log at the start of each run so you get a clean log for that run.

## Running with Docker Compose

Build and start the full stack:

```bash
cd loadtest
make build
make start
```

Run the production-traffic script against the docker stack:

```bash
make test
```

Override variables:

```bash
DURATION=10m VUS=10 make test
```

Stop everything:

```bash
make stop
```

## Failure logging

Unexpected failures are emitted as JSON Lines on stderr by the k6 script. The `Makefile` redirects them to:

- Local: `loadtest/results/unexpected-failures.jsonl`
- Docker: `loadtest/results/unexpected-failures.jsonl` (mounted from `/results` inside the k6 container)

Each log entry contains:

```json
{
  "timestamp": "2026-07-13T12:00:00.000Z",
  "scenario": "legacy_flow",
  "method": "POST",
  "url": "https://127.0.0.1:3001/data/add",
  "status": 500,
  "expected_statuses": [200],
  "request_body": "{...}",
  "response_body": "{...}",
  "response_time_ms": 42,
  "vu": 3,
  "iteration": 12,
  "tenant_id": "public",
  "base_url": "https://127.0.0.1:3001",
  "error": "unexpected_status"
}
```

Note: k6 does not allow arbitrary file writes from VU code. File output is achieved via k6's `--console-output` flag and `handleSummary`.
