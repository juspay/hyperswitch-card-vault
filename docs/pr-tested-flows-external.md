# Tested flows — EXTERNAL key manager (mock service on :5000), raw JSON (`x-tenant-id: public`)

### Health
```bash
curl -X GET http://localhost:3001/health -H 'x-tenant-id: public' -H 'content-type: application/json'
```
**Response — `HTTP 200`**
```json
{
  "message": "Health is good"
}
```

### Create entity — explicit provisioning (idempotent)
```bash
curl -X POST http://localhost:3001/entity -H 'x-tenant-id: public' -H 'content-type: application/json' \
  -d '{"entity_id":"merchant_ext"}'
```
**Response — `HTTP 200`**
```json
{
  "entity_id": "merchant_ext",
  "created_at": "2026-06-24T19:27:37.552Z"
}
```
Calling again with the same `entity_id` returns `HTTP 200` with the identical record (no new key
created in the key manager, no new row). Once provisioned, the add-card / vault flows below find the
existing entity and skip the deprecated lazy auto-create path (no `add_flow_auto_create` warning is
logged). Blank `entity_id` → `HTTP 400`; missing `entity_id` field → `HTTP 422`; missing `x-tenant-id` → `HTTP 400`.

### Add card — new (no duplication)
```bash
curl -X POST http://localhost:3001/data/add -H 'x-tenant-id: public' -H 'content-type: application/json' \
  -d '{"merchant_id":"merchant_ext","merchant_customer_id":"cust_ext","card":{"card_number":"4111111111111111","name_on_card":"John Doe","card_exp_month":"03","card_exp_year":"2030","card_brand":"VISA","card_isin":"411111","nick_name":"visa-1"},"ttl":3600}'
```
**Response — `HTTP 200`**
```json
{
  "status": "Ok",
  "payload": {
    "card_reference": "019efd86-62cd-7fe3-bf6a-bc75bcc67c9f",
    "duplication_check": null,
    "dedup": null
  }
}
```

### Add card — exact same card (dedup -> duplicated)
```bash
curl -X POST http://localhost:3001/data/add -H 'x-tenant-id: public' -H 'content-type: application/json' \
  -d '{"merchant_id":"merchant_ext","merchant_customer_id":"cust_ext","card":{"card_number":"4111111111111111","name_on_card":"John Doe","card_exp_month":"03","card_exp_year":"2030","card_brand":"VISA","card_isin":"411111","nick_name":"visa-1"},"ttl":3600}'
```
**Response — `HTTP 200`**
```json
{
  "status": "Ok",
  "payload": {
    "card_reference": "019efd86-62cd-7fe3-bf6a-bc75bcc67c9f",
    "duplication_check": "duplicated",
    "dedup": null
  }
}
```

### Add card — same number, changed metadata (-> meta_data_changed)
```bash
curl -X POST http://localhost:3001/data/add -H 'x-tenant-id: public' -H 'content-type: application/json' \
  -d '{"merchant_id":"merchant_ext","merchant_customer_id":"cust_ext","card":{"card_number":"4111111111111111","name_on_card":"Jane NEW","card_exp_month":"05","card_exp_year":"2031"},"ttl":3600}'
```
**Response — `HTTP 200`**
```json
{
  "status": "Ok",
  "payload": {
    "card_reference": "019efd86-62cd-7fe3-bf6a-bc75bcc67c9f",
    "duplication_check": "meta_data_changed",
    "dedup": null
  }
}
```

### Retrieve card — existing
```bash
curl -X POST http://localhost:3001/data/retrieve -H 'x-tenant-id: public' -H 'content-type: application/json' \
  -d '{"merchant_id":"merchant_ext","merchant_customer_id":"cust_ext","card_reference":"019efd86-62cd-7fe3-bf6a-bc75bcc67c9f"}'
```
**Response — `HTTP 200`**
```json
{
  "status": "Ok",
  "payload": {
    "card": {
      "card_number": "4111111111111111",
      "name_on_card": "John Doe",
      "card_exp_month": "03",
      "card_exp_year": "2030",
      "card_brand": "VISA",
      "card_isin": "411111",
      "nick_name": "visa-1"
    },
    "enc_card_data": null
  }
}
```

### Retrieve card — missing reference (find_by_* -> 404)
```bash
curl -X POST http://localhost:3001/data/retrieve -H 'x-tenant-id: public' -H 'content-type: application/json' \
  -d '{"merchant_id":"merchant_ext","merchant_customer_id":"cust_ext","card_reference":"does-not-exist"}'
```
**Response — `HTTP 404`**
```json
{
  "code": "TE_02",
  "message": "Requested resource not found",
  "data": null
}
```

### Fingerprint — new data+key
```bash
curl -X POST http://localhost:3001/data/fingerprint -H 'x-tenant-id: public' -H 'content-type: application/json' \
  -d '{"data":"4111111111111111","key":"hmac-key-1"}'
```
**Response — `HTTP 200`**
```json
{
  "fingerprint_id": "lw7MKEd52fZFUGN4m6jn"
}
```

### Fingerprint — same data+key (dedup -> same id)
```bash
curl -X POST http://localhost:3001/data/fingerprint -H 'x-tenant-id: public' -H 'content-type: application/json' \
  -d '{"data":"4111111111111111","key":"hmac-key-1"}'
```
**Response — `HTTP 200`**
```json
{
  "fingerprint_id": "lw7MKEd52fZFUGN4m6jn"
}
```

### Fingerprint — same data, different key (-> different id)
```bash
curl -X POST http://localhost:3001/data/fingerprint -H 'x-tenant-id: public' -H 'content-type: application/json' \
  -d '{"data":"4111111111111111","key":"hmac-key-2"}'
```
**Response — `HTTP 200`**
```json
{
  "fingerprint_id": "GRQrD0akE1kMXh1wXI9Z"
}
```

### Vault v2 — insert
```bash
curl -X POST http://localhost:3001/api/v2/vault/add -H 'x-tenant-id: public' -H 'content-type: application/json' \
  -d '{"entity_id":"entity_ext","vault_id":"vault_ext","data":{"acct":"12345","routing":"998"},"ttl":3600}'
```
**Response — `HTTP 200`**
```json
{
  "entity_id": "entity_ext",
  "vault_id": "vault_ext"
}
```

### Vault v2 — insert same again (insert_or_get, no overwrite)
```bash
curl -X POST http://localhost:3001/api/v2/vault/add -H 'x-tenant-id: public' -H 'content-type: application/json' \
  -d '{"entity_id":"entity_ext","vault_id":"vault_ext","data":{"acct":"SHOULD-NOT-OVERWRITE"},"ttl":3600}'
```
**Response — `HTTP 200`**
```json
{
  "entity_id": "entity_ext",
  "vault_id": "vault_ext"
}
```

### Vault v2 — retrieve (still original)
```bash
curl -X POST http://localhost:3001/api/v2/vault/retrieve -H 'x-tenant-id: public' -H 'content-type: application/json' \
  -d '{"entity_id":"entity_ext","vault_id":"vault_ext"}'
```
**Response — `HTTP 200`**
```json
{
  "data": {
    "acct": "12345",
    "routing": "998"
  }
}
```

### Vault v2 — upsert (overwrite)
```bash
curl -X POST http://localhost:3001/api/v2/vault/add?mode=upsert -H 'x-tenant-id: public' -H 'content-type: application/json' \
  -d '{"entity_id":"entity_ext","vault_id":"vault_ext","data":{"acct":"OVERWRITTEN"},"ttl":7200}'
```
**Response — `HTTP 200`**
```json
{
  "entity_id": "entity_ext",
  "vault_id": "vault_ext"
}
```

### Vault v2 — retrieve (overwritten)
```bash
curl -X POST http://localhost:3001/api/v2/vault/retrieve -H 'x-tenant-id: public' -H 'content-type: application/json' \
  -d '{"entity_id":"entity_ext","vault_id":"vault_ext"}'
```
**Response — `HTTP 200`**
```json
{
  "data": {
    "acct": "OVERWRITTEN"
  }
}
```

### Vault v2 — retrieve missing (find_by_* -> 404)
```bash
curl -X POST http://localhost:3001/api/v2/vault/retrieve -H 'x-tenant-id: public' -H 'content-type: application/json' \
  -d '{"entity_id":"entity_ext","vault_id":"missing"}'
```
**Response — `HTTP 404`**
```json
{
  "code": "TE_02",
  "message": "Requested resource not found",
  "data": null
}
```

### Vault v2 — delete
```bash
curl -X POST http://localhost:3001/api/v2/vault/delete -H 'x-tenant-id: public' -H 'content-type: application/json' \
  -d '{"entity_id":"entity_ext","vault_id":"vault_ext"}'
```
**Response — `HTTP 200`**
```json
{
  "entity_id": "entity_ext",
  "vault_id": "vault_ext"
}
```

### Delete card
```bash
curl -X POST http://localhost:3001/data/delete -H 'x-tenant-id: public' -H 'content-type: application/json' \
  -d '{"merchant_id":"merchant_ext","merchant_customer_id":"cust_ext","card_reference":"019efd86-62cd-7fe3-bf6a-bc75bcc67c9f"}'
```
**Response — `HTTP 200`**
```json
{
  "status": "Ok"
}
```

### Retrieve after delete (-> 404)
```bash
curl -X POST http://localhost:3001/data/retrieve -H 'x-tenant-id: public' -H 'content-type: application/json' \
  -d '{"merchant_id":"merchant_ext","merchant_customer_id":"cust_ext","card_reference":"019efd86-62cd-7fe3-bf6a-bc75bcc67c9f"}'
```
**Response — `HTTP 404`**
```json
{
  "code": "TE_02",
  "message": "Requested resource not found",
  "data": null
}
```

