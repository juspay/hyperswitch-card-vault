-- Your SQL goes here

CREATE TABLE IF NOT EXISTS reverse_lookup (
    lookup_id BYTEA NOT NULL PRIMARY KEY,
    sk_id VARCHAR(50) NOT NULL,
    pk_id VARCHAR(255) NOT NULL,
    source VARCHAR(30) NOT NULL,
    updated_by VARCHAR NOT NULL DEFAULT 'postgres_only'
);

CREATE INDEX IF NOT EXISTS lookup_id_index ON reverse_lookup (lookup_id);
