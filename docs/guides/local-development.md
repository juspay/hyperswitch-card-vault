# Local Development Guide

This guide starts Hyperswitch Card Vault locally with PostgreSQL, TLS, and the local Postman collection.

## Prerequisites

- Rust/Cargo matching `Cargo.toml`.
- PostgreSQL running on `localhost:5432`.
- Diesel CLI with Postgres support: `cargo install diesel_cli --no-default-features --features postgres`.
- OpenSSL for local certificate generation.
- Docker, when using the optional local PostgreSQL primary/replica setup.

Docker is not required for the default single-PostgreSQL local flow.

## Local Certificates

Local certificate and key material is stored under `certs/local/`, which is ignored by git.

Generated files:

- `certs/local/tls-cert.pem`: self-signed TLS certificate for `localhost` and `127.0.0.1`.
- `certs/local/tls-key.pem`: TLS private key used by the local server.
- `certs/local/locker-private-key.pem`: locker RSA private key for JWE/JWS middleware flows.
- `certs/local/locker-public-key.pem`: locker RSA public key.
- `certs/local/tenant-private-key.pem`: tenant RSA private key.
- `certs/local/tenant-public-key.pem`: tenant RSA public key.

Regenerate them if needed:

```bash
mkdir -p certs/local
openssl req -x509 -newkey rsa:2048 -nodes \
  -keyout certs/local/tls-key.pem \
  -out certs/local/tls-cert.pem \
  -sha256 -days 365 \
  -subj "/CN=localhost" \
  -addext "subjectAltName=DNS:localhost,IP:127.0.0.1"
openssl genrsa -out certs/local/locker-private-key.pem 2048
openssl rsa -in certs/local/locker-private-key.pem -pubout -out certs/local/locker-public-key.pem
openssl genrsa -out certs/local/tenant-private-key.pem 2048
openssl rsa -in certs/local/tenant-private-key.pem -pubout -out certs/local/tenant-public-key.pem
```

`config/development.toml` is configured to serve TLS with `certs/local/tls-cert.pem` and `certs/local/tls-key.pem`.

## Database Setup

The default development config uses:

```text
database: locker
username: db_user
password: db_pass
host: localhost
port: 5432
tenant: public
```

Create the local database role and database:

```bash
psql -U postgres -d postgres -c "DO \$\$ BEGIN IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'db_user') THEN CREATE ROLE db_user WITH PASSWORD 'db_pass' SUPERUSER CREATEDB CREATEROLE INHERIT LOGIN; END IF; END \$\$;"
if [ -z "$(psql -U postgres -d postgres -tAc "SELECT 1 FROM pg_database WHERE datname = 'locker'")" ]; then createdb -U postgres -O db_user locker; fi
```

Run migrations:

```bash
DATABASE_URL="postgres://db_user:db_pass@localhost:5432/locker" diesel migration run
```

## PostgreSQL Primary/Replica Setup

Use this optional Docker Compose setup when testing read-replica connectivity or primary database outage behavior.

Start the local primary, read replica, and runtime-config server:

```bash
docker compose -f docker-compose.pg-replica.yml up -d
```

The compose file exposes:

- Primary PostgreSQL: `localhost:5432`
- Read replica PostgreSQL: `localhost:5433`
- Runtime config server: `http://localhost:9091`

Run migrations against the primary:

```bash
DATABASE_URL="postgres://db_user:db_pass@localhost:5432/locker" diesel migration run
```

Verify primary and replica state:

```bash
psql "postgres://db_user:db_pass@localhost:5432/locker" -c "select pg_is_in_recovery();"
psql "postgres://db_user:db_pass@localhost:5433/locker" -c "select pg_is_in_recovery();"
```

Expected output is `f` for the primary and `t` for the replica.

Run the locker with the read replica and runtime config enabled:

```bash
LOCKER__READ_REPLICA__USERNAME=db_user \
LOCKER__READ_REPLICA__PASSWORD=db_pass \
LOCKER__READ_REPLICA__HOST=localhost \
LOCKER__READ_REPLICA__PORT=5433 \
LOCKER__READ_REPLICA__DBNAME=locker \
LOCKER__READ_REPLICA__POOL_SIZE=10 \
LOCKER__RUNTIME_CONFIG__MODE=enabled \
LOCKER__RUNTIME_CONFIG__TTL_SECONDS=5 \
LOCKER__RUNTIME_CONFIG__ENDPOINT__BASE_URL=http://localhost:9091/configs \
LOCKER__RUNTIME_CONFIG__ENDPOINT__API_KEY=dev \
cargo run --bin locker
```

The static runtime-config response is stored at `dev/runtime-config/configs/locker.use_read_replica` and enables replica routing with:

```json
{"use_replica":true}
```

Change replica routing while the locker is running by editing the same mounted file. No container restart is required because the Python server serves the latest file contents from the host directory.

Disable replica routing:

```bash
printf '%s\n' '{"key":"runtime_config","value":"{\"use_replica\":false}"}' > dev/runtime-config/configs/locker.use_read_replica
```

Enable replica routing again:

```bash
printf '%s\n' '{"key":"runtime_config","value":"{\"use_replica\":true}"}' > dev/runtime-config/configs/locker.use_read_replica
```

The locker fetches `http://localhost:9091/configs/locker.use_read_replica`. With `LOCKER__RUNTIME_CONFIG__TTL_SECONDS=5`, changes can take up to about 5 seconds to apply when the `caching` feature is enabled. Without caching, the value is fetched on demand.

Check diagnostics:

```bash
curl -k https://localhost:3001/health/diagnostics -H "x-tenant-id: public"
```

Stop only the primary while the application is still running:

```bash
docker compose -f docker-compose.pg-replica.yml stop pg-primary
```

Expected behavior:

- Primary database health and write operations fail.
- Replica health remains available while `pg-replica` is running.
- Only code paths that use replica routing can continue reading from the replica.
- This setup does not provide automatic primary failover or promotion.

Start the primary again:

```bash
docker compose -f docker-compose.pg-replica.yml start pg-primary
```

Remove the local database volumes when you want a clean setup:

```bash
docker compose -f docker-compose.pg-replica.yml down -v
```

## Run The Service

Foreground:

```bash
cargo run --bin locker
```

Background with logs:

```bash
cargo run --bin locker > target/locker-local.log 2>&1 &
```

Verify startup:

```bash
curl -k https://localhost:3001/health
```

Expected response:

```json
{"message":"Health is good"}
```

Run diagnostics:

```bash
curl -k https://localhost:3001/health/diagnostics -H "x-tenant-id: public"
```

## Postman Testing

Import this collection:

```text
docs/collection/hyperswitch-card-vault.postman_collection.json
```

Collection defaults:

- `base_url`: `https://localhost:3001`
- `tenant_id`: `public`

Because the local TLS certificate is self-signed, either disable SSL certificate verification in Postman or trust `certs/local/tls-cert.pem`.

Recommended request order:

1. `Health / Health`
2. `Health / Diagnostics`
3. `Legacy Data API / Add Card`
4. `Legacy Data API / Retrieve Card`
5. `Fingerprint API / Fingerprint`
6. `Vault API v2 / Add Data`
7. `Vault API v2 / Retrieve Data`
8. `Vault API v2 / Delete Data`

`Legacy Data API / Add Card` stores `card_reference` in collection variables for retrieve/delete requests.

## Middleware Feature Notes

The default local run does not enable the `middleware` feature, so Postman can send plain JSON request bodies.

If you run with middleware enabled, requests and responses must be JWE/JWS encrypted. Use the generated local RSA keys with the `utils` binary:

```bash
cat request.json | cargo run --bin utils -- jwe-encrypt --priv certs/local/tenant-private-key.pem --pub certs/local/locker-public-key.pem
cat response.json | cargo run --bin utils -- jwe-decrypt --priv certs/local/tenant-private-key.pem --pub certs/local/locker-public-key.pem
```

When enabling middleware, configure the locker with the locker private key and tenant public key from `certs/local/locker-private-key.pem` and `certs/local/tenant-public-key.pem`.

## Key Custodian Feature Notes

The default local run does not enable `key_custodian`. If you run with `--features key_custodian`, generate custodian keys with:

```bash
cargo run --bin utils -- master-key
```

Then set the generated encrypted master key in tenant config and use the collection's `Key Custodian (release feature)` folder to provide `key1`, `key2`, and unlock the tenant.
