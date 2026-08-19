-- Per-tenant runtime configuration store.
-- Each tenant schema gets its own `configs` table; the application seeds
-- a default `locker_runtime_config` row at startup when one is missing.
-- Both `created_at` and `updated_at` are populated by the application on
-- insert/update — no database defaults.
CREATE TABLE IF NOT EXISTS configs (
    key        VARCHAR(255) PRIMARY KEY,
    value      JSONB        NOT NULL,
    created_at TIMESTAMP    NOT NULL,
    updated_at TIMESTAMP    NOT NULL
);
