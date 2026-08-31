-- Add `updated_by` to the KV-participating tables.
-- Tracks which storage backend wrote a row (`postgres_only` or `redis_kv`).

ALTER TABLE fingerprint
    ADD COLUMN IF NOT EXISTS updated_by VARCHAR(32);

ALTER TABLE hash_table
    ADD COLUMN IF NOT EXISTS updated_by VARCHAR(32);

ALTER TABLE locker
    ADD COLUMN IF NOT EXISTS updated_by VARCHAR(32);

ALTER TABLE vault
    ADD COLUMN IF NOT EXISTS updated_by VARCHAR(32);
