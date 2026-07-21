-- Your SQL goes here

CREATE TABLE IF NOT EXISTS reverse_lookup (
    lookup_id VARCHAR NOT NULL PRIMARY KEY,
    secondary_key VARCHAR NOT NULL,
    partition_key VARCHAR NOT NULL,
    source VARCHAR NOT NULL,
    updated_by VARCHAR(32) NOT NULL
);
