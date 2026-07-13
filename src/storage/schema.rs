// @generated automatically by Diesel CLI.

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
    fingerprint (fingerprint_hash) {
        id -> Int4,
        fingerprint_hash -> Bytea,
        #[max_length = 64]
        fingerprint_id -> Varchar,
        updated_by -> Varchar,
    }
}

diesel::table! {
    hash_table (data_hash) {
        id -> Int4,
        #[max_length = 255]
        hash_id -> Varchar,
        data_hash -> Bytea,
        created_at -> Timestamp,
        updated_by -> Varchar,
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
        updated_by -> Varchar,
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
    reverse_lookup (lookup_id) {
        lookup_id -> Varchar,
        secondary_key -> Varchar,
        partition_key -> Varchar,
        source -> Varchar,
        update_by -> Varchar,
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
        updated_by -> Varchar,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    entity,
    fingerprint,
    hash_table,
    locker,
    merchant,
    reverse_lookup,
    vault,
);
