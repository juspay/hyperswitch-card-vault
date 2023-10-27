-- Your SQL goes here


CREATE TABLE hash_table (
  id SERIAL,
  hash_id VARCHAR(255) NOT NULL,
  data_hash BYTEA UNIQUE NOT NULL,
  created_at TIMESTAMP NOT NULL DEFAULT now()::TIMESTAMP,

  PRIMARY KEY (hash_id)
);


ALTER TABLE locker ADD IF NOT EXISTS hash_id VARCHAR(255) NOT NULL;
