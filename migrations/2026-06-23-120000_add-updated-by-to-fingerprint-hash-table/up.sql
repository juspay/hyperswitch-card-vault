-- Add updated_by column to track which storage scheme last wrote a row.
-- Nullable, no DEFAULT: the application stamps it from the decided storage scheme on every write.

ALTER TABLE fingerprint ADD COLUMN updated_by VARCHAR;
ALTER TABLE hash_table ADD COLUMN updated_by VARCHAR;
ALTER TABLE locker ADD COLUMN updated_by VARCHAR;
ALTER TABLE vault ADD COLUMN updated_by VARCHAR;
