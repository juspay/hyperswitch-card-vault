-- Add updated_by column to track which storage scheme last wrote a row.
-- Default 'postgres_only' is correct for all existing rows (no backfill needed).

ALTER TABLE fingerprint ADD COLUMN updated_by VARCHAR NOT NULL DEFAULT 'postgres_only';
ALTER TABLE hash_table ADD COLUMN updated_by VARCHAR NOT NULL DEFAULT 'postgres_only';
