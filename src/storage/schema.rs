// @generated automatically by Diesel CLI.

diesel::table! {
    card_info (card_isin) {
        card_isin -> Text,
        card_switch_provider -> Text,
        card_type -> Nullable<Text>,
        card_sub_type -> Nullable<Text>,
        card_sub_type_category -> Nullable<Text>,
        card_issuer_country -> Nullable<Text>,
        country_code -> Nullable<Text>,
        extended_card_type -> Nullable<Text>,
    }
}

diesel::table! {
    emi_bank_code (id) {
        id -> Int8,
        emi_bank -> Text,
        juspay_bank_code_id -> Nullable<Int8>,
        last_updated -> Nullable<Timestamp>,
    }
}

diesel::table! {
    entity (entity_id) {
        id -> Int4,
        #[max_length = 255]
        entity_id -> Varchar,
        #[max_length = 255]
        enc_key_id -> Varchar,
        created_at -> Timestamp,
    }
}

diesel::table! {
    feature (id) {
        id -> Int4,
        enabled -> Bool,
        name -> Text,
        merchant_id -> Nullable<Text>,
    }
}

diesel::table! {
    fingerprint (fingerprint_hash) {
        id -> Int4,
        fingerprint_hash -> Bytea,
        #[max_length = 64]
        fingerprint_id -> Varchar,
    }
}

diesel::table! {
    gateway_bank_emi_support (id) {
        id -> Int8,
        gateway -> Text,
        bank -> Text,
        juspay_bank_code_id -> Nullable<Int8>,
        scope -> Nullable<Text>,
    }
}

diesel::table! {
    gateway_card_info (id) {
        id -> Int8,
        isin -> Nullable<Text>,
        gateway -> Nullable<Text>,
        card_issuer_bank_name -> Nullable<Text>,
        auth_type -> Nullable<Text>,
        juspay_bank_code_id -> Nullable<Int8>,
        disabled -> Nullable<Bool>,
        validation_type -> Nullable<Text>,
        payment_method_type -> Nullable<Text>,
    }
}

diesel::table! {
    hash_table (hash_id) {
        id -> Int4,
        #[max_length = 255]
        hash_id -> Varchar,
        data_hash -> Bytea,
        created_at -> Timestamp,
    }
}

diesel::table! {
    juspay_bank_code (id) {
        id -> Int8,
        bank_code -> Text,
        bank_name -> Text,
    }
}

diesel::table! {
    locker (merchant_id, customer_id, locker_id) {
        id -> Int4,
        #[max_length = 255]
        locker_id -> Varchar,
        #[max_length = 255]
        merchant_id -> Varchar,
        #[max_length = 255]
        customer_id -> Varchar,
        enc_data -> Bytea,
        created_at -> Timestamp,
        #[max_length = 255]
        hash_id -> Varchar,
        ttl -> Nullable<Timestamp>,
    }
}

diesel::table! {
    merchant (merchant_id) {
        id -> Int4,
        #[max_length = 255]
        merchant_id -> Varchar,
        enc_key -> Bytea,
        created_at -> Timestamp,
    }
}

diesel::table! {
    merchant_gateway_account (id) {
        id -> Int8,
        account_details -> Text,
        gateway -> Text,
        merchant_id -> Text,
        payment_methods -> Nullable<Text>,
        supported_payment_flows -> Nullable<Text>,
        disabled -> Nullable<Bool>,
        reference_id -> Nullable<Text>,
        supported_currencies -> Nullable<Text>,
        gateway_identifier -> Nullable<Text>,
        gateway_type -> Nullable<Text>,
        supported_txn_type -> Nullable<Text>,
    }
}

diesel::table! {
    user_eligibility_info (id) {
        id -> Text,
        flow_type -> Text,
        identifier_name -> Text,
        identifier_value -> Text,
        provider_name -> Text,
        disabled -> Nullable<Bool>,
    }
}

diesel::table! {
    vault (entity_id, vault_id) {
        id -> Int4,
        #[max_length = 255]
        entity_id -> Varchar,
        #[max_length = 255]
        vault_id -> Varchar,
        encrypted_data -> Bytea,
        created_at -> Timestamp,
        expires_at -> Nullable<Timestamp>,
    }
}

diesel::joinable!(emi_bank_code -> juspay_bank_code (juspay_bank_code_id));
diesel::joinable!(gateway_bank_emi_support -> juspay_bank_code (juspay_bank_code_id));
diesel::joinable!(gateway_card_info -> card_info (isin));

diesel::allow_tables_to_appear_in_same_query!(
    card_info,
    emi_bank_code,
    entity,
    feature,
    fingerprint,
    gateway_bank_emi_support,
    gateway_card_info,
    hash_table,
    juspay_bank_code,
    locker,
    merchant,
    merchant_gateway_account,
    user_eligibility_info,
    vault,
);
