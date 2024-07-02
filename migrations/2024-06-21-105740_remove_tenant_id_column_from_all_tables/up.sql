-- Your SQL goes here
ALTER TABLE merchant DROP CONSTRAINT merchant_pkey, ADD CONSTRAINT merchant_pkey PRIMARY KEY (merchant_id);
ALTER TABLE merchant DROP COLUMN IF EXISTS tenant_id;

ALTER TABLE locker DROP CONSTRAINT locker_pkey, ADD CONSTRAINT locker_pkey PRIMARY KEY (merchant_id, customer_id, locker_id);
ALTER TABLE locker DROP COLUMN IF EXISTS tenant_id;


