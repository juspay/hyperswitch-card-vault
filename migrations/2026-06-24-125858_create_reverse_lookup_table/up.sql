-- Your SQL goes here

CREATE TABLE IF NOT EXISTS reverse_lookup (
    lookup_id BYTEA NOT NULL PRIMARY KEY,
    secondary_key VARCHAR NOT NULL,
    partition_key VARCHAR NOT NULL,
    source VARCHAR NOT NULL,
    update_by VARCHAR NOT NULL
);
