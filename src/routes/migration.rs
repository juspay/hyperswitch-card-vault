use crate::{
    app::AppState,
    crypto::{aes::GcmAes256, sha::Sha512, Encode},
    error::{self, ContainerError, ResultContainerExt},
    routes::data::types,
    storage::{schema, types as storage_types, HashInterface, LockerInterface, MerchantInterface},
};
use axum::{
    extract::{self, Path},
    Json,
};
use diesel::{associations::HasTable, BoolExpressionMethods, ExpressionMethods};
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use masking::{ExposeInterface, PeekInterface};

pub async fn migrate(
    extract::State(state): extract::State<AppState>,
    Path((merchant_id, customer_id, pmid)): Path<(String, String, String)>,
) -> Result<Json<MigrationResponse>, ContainerError<error::ApiError>> {
    let master_encryption = GcmAes256::new(state.config.secrets.master_key.clone());
    let merchant = state
        .db
        .find_or_create_by_merchant_id(
            &merchant_id,
            &state.config.secrets.tenant,
            &master_encryption,
        )
        .await?;

    let merchant_dek = GcmAes256::new(merchant.enc_key.expose());

    let stored_card = state
        .db
        .find_by_locker_id_merchant_id_customer_id(
            masking::Secret::new(pmid.clone()),
            &state.config.secrets.tenant,
            &merchant_id,
            &customer_id,
            &merchant_dek,
        )
        .await?;

    let raw_card =
        match serde_json::from_slice::<types::StoredData>(&stored_card.enc_data.clone().expose())
            .change_error(error::ApiError::DecodingError)?
        {
            types::StoredData::EncData(_) => {
                return Ok(Json(MigrationResponse {
                    message: "Encrypted card cannot be migrated".to_string(),
                }))
            }
            types::StoredData::CardData(card) => card,
        };

    let card_number_hash = serde_json::to_vec(&raw_card.card_number.peek())
        .change_error(error::ApiError::EncodingError)
        .and_then(|data| Ok((Sha512).encode(data)?))?;

    let optional_hash_table = state.db.find_by_data_hash(&card_number_hash).await?;

    let mut conn = state
        .db
        .get_conn()
        .await
        .change_error(error::ApiError::DatabaseError)?;

    conn.build_transaction()
        .read_write()
        .run(|conn| {
            Box::pin(async move {
                let new_hash_id = match optional_hash_table {
                    Some(hash) => hash.hash_id,
                    None => {
                        let hash_table = insert_hash(state, conn, card_number_hash).await?;

                        hash_table.hash_id
                    }
                };

                update_locker_with_new_hash_id(conn, new_hash_id, stored_card).await?;

                Ok::<(), ContainerError<error::ApiError>>(())
            })
        })
        .await?;

    // let new_hash_id = match optional_hash_table {
    //     Some(hash) => hash.hash_id,
    //     None => {
    //         let hash_table = insert_hash(state, &mut conn, card_number_hash).await?;
    //         println!("hash-id: {:?}", hash_table.hash_id);
    //         hash_table.hash_id
    //     }
    // };

    // update_locker_with_new_hash_id(&mut conn, new_hash_id, stored_card).await?;

    Ok(Json(MigrationResponse {
        message: format!(
            "Migration successful for merchant_id: {}, customer_id: {}, pmid: {}",
            merchant_id, customer_id, pmid
        ),
    }))
}

async fn update_locker_with_new_hash_id(
    conn: &mut AsyncPgConnection,
    new_hash_id: String,
    stored_card: storage_types::Locker,
) -> Result<(), ContainerError<error::LockerDBError>> {
    // return Err(error::LockerDBError::DBError.into());

    diesel::update(crate::storage::schema::locker::table)
        .filter(
            schema::locker::locker_id
                .eq(stored_card.locker_id.expose())
                .and(schema::locker::tenant_id.eq(stored_card.tenant_id))
                .and(schema::locker::merchant_id.eq(stored_card.merchant_id))
                .and(schema::locker::customer_id.eq(stored_card.customer_id)),
        )
        .set(schema::locker::hash_id.eq(new_hash_id))
        .execute(conn)
        .await
        .change_error(error::StorageError::UpdateError)?;

    Ok(())
}

#[derive(serde::Serialize)]
pub struct MigrationResponse {
    pub message: String,
}

impl From<diesel::result::Error> for ContainerError<error::ApiError> {
    fn from(_: diesel::result::Error) -> Self {
        error::ApiError::DatabaseTransactionError.into()
    }
}

async fn insert_hash(
    state: AppState,
    conn: &mut AsyncPgConnection,
    data_hash: Vec<u8>,
) -> Result<storage_types::HashTable, ContainerError<error::HashDBError>> {
    let output = state.db.find_by_data_hash(&data_hash).await?;
    match output {
        Some(inner) => Ok(inner),
        None => {
            let query = diesel::insert_into(storage_types::HashTable::table()).values(
                storage_types::HashTableNew {
                    hash_id: uuid::Uuid::new_v4().to_string(),
                    data_hash,
                },
            );

            Ok(query
                .get_result(conn)
                .await
                .change_error(error::StorageError::InsertError)?)
        }
    }
}
