-- Your SQL goes here

CREATE TABLE IF NOT EXISTS reverse_lookup (
    lookup_id BYTEA NOT NULL PRIMARY KEY,
    sk_id VARCHAR NOT NULL,
    pk_id VARCHAR NOT NULL,
    source VARCHAR NOT NULL
);

CREATE INDEX IF NOT EXISTS lookup_id_index ON reverse_lookup (lookup_id);
