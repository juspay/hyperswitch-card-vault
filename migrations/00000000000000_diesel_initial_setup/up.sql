-- This file was automatically created by Diesel to setup helper functions
-- and other internal bookkeeping. This file is safe to edit, any future
-- changes will be added to existing projects as new migrations.




-- Sets up a trigger for the given table to automatically set a column called
-- `updated_at` whenever the row is modified (unless `updated_at` was included
-- in the modified columns)
--
-- # Example
--
-- ```sql
-- CREATE TABLE users (id SERIAL PRIMARY KEY, updated_at TIMESTAMP NOT NULL DEFAULT NOW());
--
-- SELECT diesel_manage_updated_at('users');
-- ```
CREATE OR REPLACE FUNCTION diesel_manage_updated_at(_tbl regclass) RETURNS VOID AS $$
BEGIN
    EXECUTE format('CREATE TRIGGER set_updated_at BEFORE UPDATE ON %s
                    FOR EACH ROW EXECUTE PROCEDURE diesel_set_updated_at()', _tbl);
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION diesel_set_updated_at() RETURNS trigger AS $$
BEGIN
    IF (
        NEW IS DISTINCT FROM OLD AND
        NEW.updated_at IS NOT DISTINCT FROM OLD.updated_at
    ) THEN
        NEW.updated_at := current_timestamp;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Disable foreign key constraints
SET CONSTRAINTS ALL DEFERRED;

-- DROP DATABASE IF EXISTS jdb;
-- CREATE DATABASE jdb;
-- \c jdb; -- Uncomment if running in psql

DROP TABLE IF EXISTS card_info;
CREATE TABLE card_info (
    card_isin TEXT PRIMARY KEY,
    card_switch_provider TEXT NOT NULL,
    card_type TEXT,
    card_sub_type TEXT,
    card_sub_type_category TEXT,
    card_issuer_country TEXT,
    country_code TEXT,
    extended_card_type TEXT
);

INSERT INTO card_info VALUES
('ISIN12345', 'ProviderA', 'Credit', 'Gold', 'Premium', 'US', 'USA', 'ExtendedA'),
('ISIN67890', 'ProviderB', 'Debit', 'Platinum', 'Basic', 'IN', 'IND', 'ExtendedB');

DROP TABLE IF EXISTS gateway_card_info;
CREATE TABLE gateway_card_info (
    id BIGSERIAL PRIMARY KEY,
    isin TEXT REFERENCES card_info(card_isin),
    gateway TEXT,
    card_issuer_bank_name TEXT,
    auth_type TEXT,
    juspay_bank_code_id BIGINT,
    disabled BOOLEAN,
    validation_type TEXT,
    payment_method_type TEXT
);

INSERT INTO gateway_card_info (isin, gateway, card_issuer_bank_name, auth_type, juspay_bank_code_id, disabled, validation_type, payment_method_type) VALUES
('ISIN12345', 'GatewayX', 'BankA', '2FA', 1, FALSE, 'Strict', 'CreditCard'),
('ISIN67890', 'GatewayY', 'BankB', 'OTP', 2, TRUE, 'Relaxed', 'DebitCard');

DROP TABLE IF EXISTS juspay_bank_code;
CREATE TABLE juspay_bank_code (
    id BIGINT PRIMARY KEY,
    bank_code TEXT NOT NULL,
    bank_name TEXT NOT NULL
);

INSERT INTO juspay_bank_code VALUES
(1, 'BANK1', 'Bank A'),
(2, 'BANK2', 'Bank B');

DROP TABLE IF EXISTS emi_bank_code;
CREATE TABLE emi_bank_code (
    id BIGSERIAL PRIMARY KEY,
    emi_bank TEXT NOT NULL,
    juspay_bank_code_id BIGINT REFERENCES juspay_bank_code(id),
    last_updated TIMESTAMP DEFAULT NOW()
);

INSERT INTO emi_bank_code (emi_bank, juspay_bank_code_id, last_updated) VALUES
('EmiBankA', 1, NOW()),
('EmiBankB', 2, NOW());

DROP TABLE IF EXISTS feature;
CREATE TABLE feature (
    id SERIAL PRIMARY KEY,
    enabled BOOLEAN NOT NULL,
    name TEXT NOT NULL,
    merchant_id TEXT
);

INSERT INTO feature (enabled, name, merchant_id) VALUES
(TRUE, 'FeatureA', 'Merchant123'),
(FALSE, 'FeatureB', 'Merchant456');

DROP TABLE IF EXISTS merchant_gateway_account;
CREATE TABLE merchant_gateway_account (
    id BIGSERIAL PRIMARY KEY,
    account_details TEXT NOT NULL,
    gateway TEXT NOT NULL,
    merchant_id TEXT NOT NULL,
    payment_methods TEXT,
    supported_payment_flows TEXT,
    disabled BOOLEAN,
    reference_id TEXT,
    supported_currencies TEXT,
    gateway_identifier TEXT,
    gateway_type TEXT,
    supported_txn_type TEXT
);

INSERT INTO merchant_gateway_account (account_details, gateway, merchant_id, payment_methods, supported_payment_flows, disabled, reference_id, supported_currencies, gateway_identifier, gateway_type, supported_txn_type) VALUES
('AccountDetailsA', 'GatewayX', 'Merchant123', 'Credit', 'Online', FALSE, 'Ref123', 'USD', 'IdentifierX', 'TypeX', 'TxnTypeA'),
('AccountDetailsB', 'GatewayY', 'Merchant456', 'Debit', 'Offline', TRUE, 'Ref456', 'INR', 'IdentifierY', 'TypeY', 'TxnTypeB');

DROP TABLE IF EXISTS user_eligibility_info;
CREATE TABLE user_eligibility_info (
    id TEXT PRIMARY KEY,
    flow_type TEXT NOT NULL,
    identifier_name TEXT NOT NULL,
    identifier_value TEXT NOT NULL,
    provider_name TEXT NOT NULL,
    disabled BOOLEAN
);

INSERT INTO user_eligibility_info VALUES
('ID123', 'FlowX', 'Email', 'user@example.com', 'ProviderA', FALSE),
('ID456', 'FlowY', 'Phone', '1234567890', 'ProviderB', TRUE);

DROP TABLE IF EXISTS gateway_bank_emi_support;
CREATE TABLE gateway_bank_emi_support (
    id BIGSERIAL PRIMARY KEY,
    gateway TEXT NOT NULL,
    bank TEXT NOT NULL,
    juspay_bank_code_id BIGINT REFERENCES juspay_bank_code(id),
    scope TEXT
);

INSERT INTO gateway_bank_emi_support (gateway, bank, juspay_bank_code_id, scope) VALUES
('GatewayX', 'BankA', 1, 'Global'),
('GatewayY', 'BankB', 2, 'Regional');

-- Enable foreign key constraints
SET CONSTRAINTS ALL IMMEDIATE;
