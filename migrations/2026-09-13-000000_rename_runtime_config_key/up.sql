-- Rename the runtime config row key `locker_runtime_config` -> `kv_config`.
--
-- Runtime configs are now a compile-time registry with one `configs` row per config,
-- each keyed by its own name. The KV/replica config becomes `kv_config`; future
-- configs get their own keys alongside it.
--
-- MUST run on every tenant schema BEFORE the new binary starts. A new pod that boots
-- first seeds a fresh `kv_config` row from the static config, after which the guard
-- below skips the rename and the DELETE drops the row holding the live state --
-- silently resetting KV to the seeded value.
--
-- The NOT EXISTS guard plus the DELETE make this idempotent, and safe in the case
-- where both rows exist (the already-present `kv_config` row wins).
UPDATE configs
   SET key = 'kv_config'
 WHERE key = 'locker_runtime_config'
   AND NOT EXISTS (SELECT 1 FROM configs WHERE key = 'kv_config');

DELETE FROM configs WHERE key = 'locker_runtime_config';
