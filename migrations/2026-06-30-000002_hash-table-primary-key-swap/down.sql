SET LOCAL lock_timeout = '2s';
SET LOCAL statement_timeout = '5s';

ALTER TABLE hash_table
    DROP CONSTRAINT hash_table_pkey,
    DROP CONSTRAINT hash_table_hash_id_key,
    ADD CONSTRAINT hash_table_pkey
        PRIMARY KEY (hash_id),
    ADD CONSTRAINT hash_table_data_hash_key
        UNIQUE (data_hash);

-- In production environments with live traffic, run these index creations with CONCURRENTLY.
CREATE UNIQUE INDEX IF NOT EXISTS hash_table_hash_id_key_idx
ON hash_table (hash_id);

CREATE UNIQUE INDEX IF NOT EXISTS hash_table_data_hash_pkey_idx
ON hash_table (data_hash);
