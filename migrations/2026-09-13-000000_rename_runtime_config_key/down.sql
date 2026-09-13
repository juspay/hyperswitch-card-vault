-- Restore the pre-registry runtime config row key `kv_config` -> `locker_runtime_config`.
UPDATE configs
   SET key = 'locker_runtime_config'
 WHERE key = 'kv_config'
   AND NOT EXISTS (SELECT 1 FROM configs WHERE key = 'locker_runtime_config');

DELETE FROM configs WHERE key = 'kv_config';
