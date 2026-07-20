CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS locker_hash_id_merchant_id_customer_id_idx ON locker (hash_id, merchant_id, customer_id);
