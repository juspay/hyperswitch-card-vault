-- Your SQL goes here

CREATE TABLE fingerprint (
  id SERIAL,
  card_hash BYTEA UNIQUE NOT NULL,
  card_fingerprint VARCHAR(255) NOT NULL,
  PRIMARY KEY (card_hash)
);