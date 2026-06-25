-- Add updated_by column to track which storage scheme last wrote a row.

ALTER TABLE reverse_lookup ADD COLUMN updated_by VARCHAR NOT NULL DEFAULT 'postgres_only';
