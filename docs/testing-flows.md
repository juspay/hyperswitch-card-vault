# Testing the data / vault / fingerprint flows

The HTTP API is identical whether the server encrypts via the **internal** key
manager (master key, no external service) or the **external** key manager
(encryption service). So `scripts/smoke-test.sh` validates both — only the way
you *run the server* changes.

Prerequisites: Postgres up, migrations applied, `curl`, `jq`.

```bash
# apply migrations (uses DATABASE_URL or diesel.toml)
diesel migration run
```

Both modes assume **no JWE/JWS transport middleware** so you can send raw JSON.
That means: do **not** enable the `middleware` feature for local testing. (With
`middleware`, request bodies must be JWE-encrypted + JWS-signed.)

---

## Mode A — internal key manager (no encryption service)  ← start here

`config/development.toml` already has:

```toml
[external_key_manager]
mode = "disabled"        # → InternalKeyManager, encrypts DEKs with tenant master_key
```

Run with the default feature set (no `middleware`, no `external_key_manager`):

```bash
cargo run --bin locker
# serves on 127.0.0.1:3001, tenant: public
```

Covers: add/retrieve/delete card, dedup + duplication check, v2 vault
insert/get/upsert/delete, fingerprint dedup — i.e. the whole data plane, using
the `merchant` table for DEKs.

Run the suite:

```bash
BASE=http://localhost:3001 TENANT=public ./scripts/smoke-test.sh
RUN_EXPIRY=1 ./scripts/smoke-test.sh        # also exercise TTL expiry (slow)
```

---

## Mode B — external key manager (encryption service)

Use this to additionally cover the **`entity` table** path
(`find_by_entity_id` / `find_or_create_entity` / `ExternalCryptoManager`), the
`/key/transfer` migration, and #171's read-replica routing for entity reads.

1. Point the locker at the encryption service in `config/development.toml`:

   ```toml
   [external_key_manager]
   mode = "enabled"
   url  = "http://localhost:5000"
   # or, with mTLS:
   # mode    = "enabled_with_mtls"
   # url     = "https://encryption-service:5000"
   # ca_cert = "<base64/pem ca cert>"
   ```

2. Build/run with the feature compiled in:

   ```bash
   cargo run --bin locker --features external_key_manager
   ```

3. The service at `url` must implement the 4 endpoints the locker calls
   (see `src/crypto/keymanager/external_keymanager.rs` for exact request/response
   shapes):

   | Endpoint | Used by |
   |---|---|
   | `POST {url}/key/create`  | first store for an entity (`find_or_create_entity`) |
   | `POST {url}/key/transfer`| `/key/transfer` merchant-key migration |
   | `POST {url}/data/encrypt`| every add/store |
   | `POST {url}/data/decrypt`| every retrieve |

   Use Hyperswitch's encryption / key-manager service, or a stub implementing
   those 4 endpoints. Confirm wiring via `GET /health/diagnostics` →
   `keymanager_status: "Working"`.

4. Run the **same** suite (no changes):

   ```bash
   ./scripts/smoke-test.sh
   ```

---

## What the suite asserts (TC ↔ refactor behaviour)

| TC | Flow | Validates |
|---|---|---|
| 1.1–1.3 | add card | new → `null`; same → `duplicated`; metadata changed → `meta_data_changed` (dedup path) |
| 1.2 / 5.2 | add / vault insert | `insert_or_get` returns existing row on duplicate key |
| 2.2 / 2.3 / 5.5 | retrieve | missing row → **404** (`find_by_*` erroring) |
| 3.1 / 3.2 / 5.6 / 5.7 | delete | idempotent (missing → still 200) |
| 4.1–4.3 | fingerprint | HMAC dedup: same data+key → same id; different key → different id |
| 5.3 | vault upsert | `upsert` overwrites (vs `insert_or_get`) |
| 1.7 / 1.8 / 1.9 / 4.6 | validation | invalid card / past TTL / missing tenant / bad fingerprint-id → 400 |
| 2.4 | retrieve (RUN_EXPIRY=1) | expired card → 404 + async delete |

Exit code is non-zero if any case fails, so it drops into CI as-is.
