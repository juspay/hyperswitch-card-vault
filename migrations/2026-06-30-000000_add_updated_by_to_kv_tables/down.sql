-- Drop the `updated_by` provenance column from the KV-participating tables.

ALTER TABLE fingerprint DROP COLUMN IF EXISTS updated_by;
ALTER TABLE hash_table   DROP COLUMN IF EXISTS updated_by;
ALTER TABLE locker       DROP COLUMN IF EXISTS updated_by;
ALTER TABLE vault        DROP COLUMN IF EXISTS updated_by;
