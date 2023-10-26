-- This file should undo anything in `up.sql`

DROP TABLE hash_table;


ALTER TABLE locker DROP COLUMN IF EXISTS hash_id;
