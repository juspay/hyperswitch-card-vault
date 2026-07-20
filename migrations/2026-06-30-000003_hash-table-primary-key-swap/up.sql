SET LOCAL lock_timeout = '2s';
SET LOCAL statement_timeout = '5s';

ALTER TABLE hash_table
    DROP CONSTRAINT hash_table_pkey,
    DROP CONSTRAINT hash_table_data_hash_key,
    ADD CONSTRAINT hash_table_pkey
        PRIMARY KEY USING INDEX hash_table_data_hash_pkey_idx,
    ADD CONSTRAINT hash_table_hash_id_key
        UNIQUE USING INDEX hash_table_hash_id_key_idx;
