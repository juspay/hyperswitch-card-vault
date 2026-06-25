-- Remove the updated_by column added for KV tracking.

ALTER TABLE reverse_lookup DROP COLUMN IF EXISTS updated_by;
