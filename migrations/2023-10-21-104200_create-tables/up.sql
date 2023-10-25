-- Your SQL goes here

CREATE TABLE merchant (
  id SERIAL,
  tenant_id VARCHAR(255) NOT NULL, 
  merchant_id VARCHAR(255) NOT NULL,
  enc_key BYTEA NOT NULL,
  created_at TIMESTAMP NOT NULL DEFAULT now()::TIMESTAMP,
  
  PRIMARY KEY (tenant_id, merchant_id)
);

CREATE TABLE locker (
  id SERIAL,
  locker_id VARCHAR(255) NOT NULL,
  tenant_id VARCHAR(255) NOT NULL, 
  merchant_id VARCHAR(255) NOT NULL, 
  customer_id VARCHAR(255) NOT NULL,
  enc_data BYTEA NOT NULL,
  created_at TIMESTAMP NOT NULL DEFAULT now()::TIMESTAMP,

  PRIMARY KEY (tenant_id, merchant_id, customer_id, locker_id)
)
