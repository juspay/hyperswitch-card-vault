// @generated automatically by Diesel CLI.

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
    locker (tenant_id, merchant_id, customer_id, locker_id) {
        id -> Int4,
        #[max_length = 255]
        locker_id -> Varchar,
        #[max_length = 255]
        tenant_id -> Varchar,
        #[max_length = 255]
        merchant_id -> Varchar,
        #[max_length = 255]
        customer_id -> Varchar,
        enc_data -> Bytea,
        created_at -> Timestamp,
        #[max_length = 255]
        hash_id -> Varchar,
    }
}

diesel::table! {
    merchant (tenant_id, merchant_id) {
        id -> Int4,
        #[max_length = 255]
        tenant_id -> Varchar,
        #[max_length = 255]
        merchant_id -> Varchar,
        enc_key -> Bytea,
        created_at -> Timestamp,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    hash_table,
    locker,
    merchant,
);
