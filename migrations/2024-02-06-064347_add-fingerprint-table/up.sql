-- Your SQL goes here

CREATE TABLE fingerprint (
  id SERIAL,
  card_hash BYTEA UNIQUE NOT NULL,
  card_fingerprint VARCHAR(64) NOT NULL,
  PRIMARY KEY (card_hash)
);