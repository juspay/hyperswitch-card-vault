-- Add `updated_by` to the KV-participating tables.
--
-- Tracks which storage backend wrote a row (`postgres_only` or `redis_kv`)
-- so the drainer and read paths can reason about provenance.
-- `schema.rs` already declares these columns; this migration brings existing
-- databases in line (it was missing from the original KV PR #177).
--
-- `IF NOT EXISTS` guards against drift on databases where the column was added
-- manually before this migration landed.

ALTER TABLE fingerprint
    ADD COLUMN IF NOT EXISTS updated_by VARCHAR;

ALTER TABLE hash_table
    ADD COLUMN IF NOT EXISTS updated_by VARCHAR;

ALTER TABLE locker
    ADD COLUMN IF NOT EXISTS updated_by VARCHAR;

ALTER TABLE vault
    ADD COLUMN IF NOT EXISTS updated_by VARCHAR;
