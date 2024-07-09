-- Your SQL goes here

ALTER TABLE fingerprint RENAME COLUMN card_fingerprint TO fingerprint_id;
ALTER TABLE fingerprint RENAME COLUMN card_hash TO fingerprint_hash;

CREATE TABLE IF NOT EXISTS vault (
    id SERIAL,
    entity_id VARCHAR(255) NOT NULL, 
    vault_id VARCHAR(255) NOT NULL,
    encrypted_data BYTEA NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT now()::TIMESTAMP,
    expires_at TIMESTAMP DEFAULT NULL,
    
    PRIMARY KEY (entity_id, vault_id)
);

CREATE TABLE IF NOT EXISTS entity (
    id SERIAL,
    entity_id VARCHAR(255) NOT NULL,
    enc_key_id VARCHAR(255) NOT NULL,

    PRIMARY KEY (entity_id)
);