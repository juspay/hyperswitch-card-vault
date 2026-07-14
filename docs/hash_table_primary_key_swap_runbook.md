# Production Runbook: Swap `hash_table` Primary Key and Unique Constraint

## Goal

Change `hash_table` from:

```sql
PRIMARY KEY (hash_id)
UNIQUE (data_hash)
```

to:

```sql
PRIMARY KEY (data_hash)
UNIQUE (hash_id)
```

The application currently queries `hash_table` using `data_hash`, then uses the returned `hash_id` to query the `locker` table. This target schema is valid as long as `hash_id` remains `UNIQUE NOT NULL`, especially if other tables reference it through foreign keys.

## Safety Principles

- Build new indexes with `CREATE INDEX CONCURRENTLY` so normal reads and writes can continue.
- Keep the final constraint swap as a short metadata operation.
- Use a low `lock_timeout` so the migration fails instead of blocking production traffic.
- Retry the final swap if it fails due to lock contention.
- Do not drop the unique protection on either column until replacement constraints are ready.

## Pre-Checks

Run these checks before production deployment.

### 1. Confirm Current Constraints

```sql
\d+ hash_table
```

Expected current state:

```text
hash_table_pkey PRIMARY KEY, btree (hash_id)
hash_table_data_hash_key UNIQUE CONSTRAINT, btree (data_hash)
```

### 2. Check for Duplicate or Null Values

These should return no rows, except the null check should return `0`.

```sql
SELECT data_hash, COUNT(*)
FROM hash_table
GROUP BY data_hash
HAVING COUNT(*) > 1;

SELECT hash_id, COUNT(*)
FROM hash_table
GROUP BY hash_id
HAVING COUNT(*) > 1;

SELECT COUNT(*) AS null_count
FROM hash_table
WHERE data_hash IS NULL OR hash_id IS NULL;
```

### 3. Check Foreign Keys Referencing `hash_table`

```sql
SELECT
    conrelid::regclass AS referencing_table,
    conname AS fk_name,
    pg_get_constraintdef(oid) AS definition
FROM pg_constraint
WHERE confrelid = 'hash_table'::regclass
  AND contype = 'f';
```

If this returns no rows, use the standard migration path below.

If this returns rows, do not assume the standard constraint swap will work. PostgreSQL may prevent dropping `hash_table_pkey` because existing foreign keys can depend on that specific primary key constraint/index. Use the FK-aware migration path instead.

### 4. Confirm No Invalid Leftover Indexes Exist

If a previous `CREATE INDEX CONCURRENTLY` failed, PostgreSQL may leave an invalid index behind.

```sql
SELECT
    indexrelid::regclass AS index_name,
    indisvalid,
    indisready
FROM pg_index
WHERE indrelid = 'hash_table'::regclass;
```

If either target index exists and is invalid, drop it concurrently before retrying:

```sql
DROP INDEX CONCURRENTLY IF EXISTS hash_table_hash_id_key_idx;
DROP INDEX CONCURRENTLY IF EXISTS hash_table_data_hash_pkey_idx;
```

## Production Migration

Use this path only if no foreign keys reference `hash_table`.

### Step 1. Create Replacement Unique Index on `hash_id`

Run outside a transaction. `CREATE INDEX CONCURRENTLY` cannot run inside a transaction block.

```sql
CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS hash_table_hash_id_key_idx
ON hash_table (hash_id);
```

### Step 2. Create Replacement Unique Index on `data_hash`

Run outside a transaction.

```sql
CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS hash_table_data_hash_pkey_idx
ON hash_table (data_hash);
```

### Step 3. Validate Replacement Indexes

Both indexes must show `indisvalid = true` and `indisready = true`.

```sql
SELECT
    indexrelid::regclass AS index_name,
    indisvalid,
    indisready
FROM pg_index
WHERE indexrelid IN (
    'hash_table_hash_id_key_idx'::regclass,
    'hash_table_data_hash_pkey_idx'::regclass
);
```

### Step 4. Short Constraint Swap

Run this during a quieter traffic window. This takes an `ACCESS EXCLUSIVE` lock briefly. The low `lock_timeout` prevents the command from waiting too long and blocking production traffic.

```sql
BEGIN;

SET LOCAL lock_timeout = '2s';
SET LOCAL statement_timeout = '5s';

ALTER TABLE hash_table
    DROP CONSTRAINT hash_table_pkey,
    DROP CONSTRAINT hash_table_data_hash_key,
    ADD CONSTRAINT hash_table_pkey
        PRIMARY KEY USING INDEX hash_table_data_hash_pkey_idx,
    ADD CONSTRAINT hash_table_hash_id_key
        UNIQUE USING INDEX hash_table_hash_id_key_idx;

COMMIT;
```

If this fails with a lock timeout, retry Step 4 later. Do not increase `lock_timeout` aggressively unless temporary write blocking is acceptable.

## FK-Aware Production Migration

Use this path if any table has a foreign key referencing `hash_table(hash_id)`.

The exact FK statements depend on the referencing tables and original FK definitions. Generate them from the pre-check query and test the full migration in staging with production-like schema before running it in production.

### Step 1. Create Replacement Indexes Concurrently

Run outside a transaction.

```sql
CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS hash_table_hash_id_key_idx
ON hash_table (hash_id);

CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS hash_table_data_hash_pkey_idx
ON hash_table (data_hash);
```

### Step 2. Attach `UNIQUE (hash_id)` Before Dropping the Primary Key

This creates a durable unique constraint for `hash_id` before the primary key is moved to `data_hash`.

```sql
BEGIN;

SET LOCAL lock_timeout = '2s';
SET LOCAL statement_timeout = '5s';

ALTER TABLE hash_table
    ADD CONSTRAINT hash_table_hash_id_key
        UNIQUE USING INDEX hash_table_hash_id_key_idx;

COMMIT;
```

If this fails with a lock timeout, retry later.

### Step 3. Replace Referencing Foreign Keys and Swap the Primary Key

For every FK returned by the pre-check, prepare equivalent `ADD CONSTRAINT ... FOREIGN KEY ... REFERENCES hash_table(hash_id) NOT VALID` statements.

Then run one short transaction that:

- Drops the existing referencing FKs.
- Drops `hash_table_pkey`.
- Drops `hash_table_data_hash_key`.
- Adds the new primary key on `data_hash`.
- Re-adds the referencing FKs as `NOT VALID`.

Template:

```sql
BEGIN;

SET LOCAL lock_timeout = '2s';
SET LOCAL statement_timeout = '5s';

-- Repeat for each referencing table/FK from the pre-check.
ALTER TABLE <referencing_table>
    DROP CONSTRAINT <old_fk_name>;

ALTER TABLE hash_table
    DROP CONSTRAINT hash_table_pkey,
    DROP CONSTRAINT hash_table_data_hash_key,
    ADD CONSTRAINT hash_table_pkey
        PRIMARY KEY USING INDEX hash_table_data_hash_pkey_idx;

-- Repeat for each dropped FK. Preserve the original ON UPDATE/ON DELETE behavior.
ALTER TABLE <referencing_table>
    ADD CONSTRAINT <fk_name>
        FOREIGN KEY (<referencing_column>)
        REFERENCES hash_table(hash_id)
        NOT VALID;

COMMIT;
```

`NOT VALID` avoids scanning the referencing table inside the lock-sensitive transaction. New writes are still checked by the FK after it is added.

### Step 4. Validate Recreated Foreign Keys

Run each validation separately after the swap. Validation does not require the same heavy lock as creating a fully validated FK immediately.

```sql
ALTER TABLE <referencing_table>
    VALIDATE CONSTRAINT <fk_name>;
```

### Step 5. Confirm All FKs Are Valid

```sql
SELECT
    conrelid::regclass AS referencing_table,
    conname AS fk_name,
    convalidated,
    pg_get_constraintdef(oid) AS definition
FROM pg_constraint
WHERE confrelid = 'hash_table'::regclass
  AND contype = 'f';
```

## Post-Checks

### 1. Confirm Final Constraints

```sql
\d+ hash_table
```

Expected final state:

```text
hash_table_pkey PRIMARY KEY, btree (data_hash)
hash_table_hash_id_key UNIQUE CONSTRAINT, btree (hash_id)
```

### 2. Confirm Application Lookup Path

Verify the normal application flow still works:

```text
data_hash -> hash_table -> hash_id -> locker
```

### 3. Confirm Foreign Keys

If any table references `hash_table(hash_id)`, confirm the FK is still valid:

```sql
SELECT
    conrelid::regclass AS referencing_table,
    conname AS fk_name,
    convalidated,
    pg_get_constraintdef(oid) AS definition
FROM pg_constraint
WHERE confrelid = 'hash_table'::regclass
  AND contype = 'f';
```

## Rollback

If the final swap has not run yet, rollback is simple:

```sql
DROP INDEX CONCURRENTLY IF EXISTS hash_table_hash_id_key_idx;
DROP INDEX CONCURRENTLY IF EXISTS hash_table_data_hash_pkey_idx;
```

If the final swap has already completed and you need to revert to the previous schema, use the same safe pattern in reverse.

### 1. Create Replacement Indexes Concurrently

```sql
CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS hash_table_hash_id_pkey_idx
ON hash_table (hash_id);

CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS hash_table_data_hash_key_idx
ON hash_table (data_hash);
```

### 2. Swap Constraints Back

```sql
BEGIN;

SET LOCAL lock_timeout = '2s';
SET LOCAL statement_timeout = '5s';

ALTER TABLE hash_table
    DROP CONSTRAINT hash_table_pkey,
    DROP CONSTRAINT hash_table_hash_id_key,
    ADD CONSTRAINT hash_table_pkey
        PRIMARY KEY USING INDEX hash_table_hash_id_pkey_idx,
    ADD CONSTRAINT hash_table_data_hash_key
        UNIQUE USING INDEX hash_table_data_hash_key_idx;

COMMIT;
```

## Notes

- A foreign key does not have to reference a primary key in PostgreSQL. It can reference a `UNIQUE NOT NULL` column.
- `data_hash` is `bytea`, so the primary key index may be larger than a text or integer key. In this table, a unique index on `data_hash` already exists, so the storage and lookup impact should be similar to the current state.
- The final `ALTER TABLE` is the only step expected to briefly block writes. Use retries instead of waiting indefinitely for locks.
