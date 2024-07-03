-- This file should undo anything in `up.sql`

ALTER TABLE fingerprint RENAME COLUMN fingerprint_id TO card_fingerprint;
ALTER TABLE fingerprint RENAME COLUMN fingerprint_hash TO card_hash;

DROP TABLE IF EXISTS vault;
DROP TABLE IF EXISTS entity;