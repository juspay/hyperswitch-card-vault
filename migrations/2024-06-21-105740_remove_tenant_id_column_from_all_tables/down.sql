-- This file should undo anything in `up.sql`

ALTER TABLE merchant ADD COLUMN IF NOT EXISTS tenant_id VARCHAR(255) NOT NULL;
ALTER TABLE merchant DROP CONSTRAINT merchant_pkey, ADD CONSTRAINT merchant_pkey PRIMARY KEY (tenant_id, merchant_id);

ALTER TABLE locker ADD COLUMN IF NOT EXISTS tenant_id VARCHAR(255) NOT NULL;
ALTER TABLE locker DROP CONSTRAINT locker_pkey, ADD CONSTRAINT locker_pkey PRIMARY KEY (tenant_id, merchant_id, customer_id, locker_id);