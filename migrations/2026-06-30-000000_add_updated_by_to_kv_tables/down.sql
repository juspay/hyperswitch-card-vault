-- This file should undo anything in `up.sql`
--
-- Drop the `updated_by` provenance column added to the KV-participating tables.
-- Guarded with `IF EXISTS` so the revert is idempotent even if the column was
-- already absent.

ALTER TABLE fingerprint DROP COLUMN IF EXISTS updated_by;
ALTER TABLE hash_table   DROP COLUMN IF EXISTS updated_by;
ALTER TABLE locker       DROP COLUMN IF EXISTS updated_by;
ALTER TABLE vault        DROP COLUMN IF EXISTS updated_by;
