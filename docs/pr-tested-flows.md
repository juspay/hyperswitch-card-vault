# Tested flows — internal key manager, raw JSON (`x-tenant-id: public`)

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

### Add card — new (no duplication)
```bash
curl -X POST http://localhost:3001/data/add -H 'x-tenant-id: public' -H 'content-type: application/json' \
  -d '{"merchant_id":"merchant_demo","merchant_customer_id":"cust_demo","card":{"card_number":"4111111111111111","name_on_card":"John Doe","card_exp_month":"03","card_exp_year":"2030","card_brand":"VISA","card_isin":"411111","nick_name":"visa-1"},"ttl":3600}'
```
**Response — `HTTP 200`**
```json
{
  "status": "Ok",
  "payload": {
    "card_reference": "019efd7d-5e8a-7bc2-ab0a-3f731545ae1c",
    "duplication_check": null,
    "dedup": null
  }
}
```

### Add card — exact same card (dedup -> duplicated)
```bash
curl -X POST http://localhost:3001/data/add -H 'x-tenant-id: public' -H 'content-type: application/json' \
  -d '{"merchant_id":"merchant_demo","merchant_customer_id":"cust_demo","card":{"card_number":"4111111111111111","name_on_card":"John Doe","card_exp_month":"03","card_exp_year":"2030","card_brand":"VISA","card_isin":"411111","nick_name":"visa-1"},"ttl":3600}'
```
**Response — `HTTP 200`**
```json
{
  "status": "Ok",
  "payload": {
    "card_reference": "019efd7d-5e8a-7bc2-ab0a-3f731545ae1c",
    "duplication_check": "duplicated",
    "dedup": null
  }
}
```

### Add card — same number, changed metadata (-> meta_data_changed)
```bash
curl -X POST http://localhost:3001/data/add -H 'x-tenant-id: public' -H 'content-type: application/json' \
  -d '{"merchant_id":"merchant_demo","merchant_customer_id":"cust_demo","card":{"card_number":"4111111111111111","name_on_card":"Jane NEW","card_exp_month":"05","card_exp_year":"2031"},"ttl":3600}'
```
**Response — `HTTP 200`**
```json
{
  "status": "Ok",
  "payload": {
    "card_reference": "019efd7d-5e8a-7bc2-ab0a-3f731545ae1c",
    "duplication_check": "meta_data_changed",
    "dedup": null
  }
}
```

### Retrieve card — existing
```bash
curl -X POST http://localhost:3001/data/retrieve -H 'x-tenant-id: public' -H 'content-type: application/json' \
  -d '{"merchant_id":"merchant_demo","merchant_customer_id":"cust_demo","card_reference":"019efd7d-5e8a-7bc2-ab0a-3f731545ae1c"}'
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
  -d '{"merchant_id":"merchant_demo","merchant_customer_id":"cust_demo","card_reference":"does-not-exist"}'
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
  -d '{"entity_id":"entity_demo","vault_id":"vault_demo","data":{"acct":"12345","routing":"998"},"ttl":3600}'
```
**Response — `HTTP 200`**
```json
{
  "entity_id": "entity_demo",
  "vault_id": "vault_demo"
}
```

### Vault v2 — insert same again (insert_or_get, no overwrite)
```bash
curl -X POST http://localhost:3001/api/v2/vault/add -H 'x-tenant-id: public' -H 'content-type: application/json' \
  -d '{"entity_id":"entity_demo","vault_id":"vault_demo","data":{"acct":"SHOULD-NOT-OVERWRITE"},"ttl":3600}'
```
**Response — `HTTP 200`**
```json
{
  "entity_id": "entity_demo",
  "vault_id": "vault_demo"
}
```

### Vault v2 — retrieve (still original)
```bash
curl -X POST http://localhost:3001/api/v2/vault/retrieve -H 'x-tenant-id: public' -H 'content-type: application/json' \
  -d '{"entity_id":"entity_demo","vault_id":"vault_demo"}'
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
  -d '{"entity_id":"entity_demo","vault_id":"vault_demo","data":{"acct":"OVERWRITTEN"},"ttl":7200}'
```
**Response — `HTTP 200`**
```json
{
  "entity_id": "entity_demo",
  "vault_id": "vault_demo"
}
```

### Vault v2 — retrieve (overwritten)
```bash
curl -X POST http://localhost:3001/api/v2/vault/retrieve -H 'x-tenant-id: public' -H 'content-type: application/json' \
  -d '{"entity_id":"entity_demo","vault_id":"vault_demo"}'
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
  -d '{"entity_id":"entity_demo","vault_id":"missing"}'
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
  -d '{"entity_id":"entity_demo","vault_id":"vault_demo"}'
```
**Response — `HTTP 200`**
```json
{
  "entity_id": "entity_demo",
  "vault_id": "vault_demo"
}
```

### Delete card
```bash
curl -X POST http://localhost:3001/data/delete -H 'x-tenant-id: public' -H 'content-type: application/json' \
  -d '{"merchant_id":"merchant_demo","merchant_customer_id":"cust_demo","card_reference":"019efd7d-5e8a-7bc2-ab0a-3f731545ae1c"}'
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
  -d '{"merchant_id":"merchant_demo","merchant_customer_id":"cust_demo","card_reference":"019efd7d-5e8a-7bc2-ab0a-3f731545ae1c"}'
```
**Response — `HTTP 404`**
```json
{
  "code": "TE_02",
  "message": "Requested resource not found",
  "data": null
}
```

