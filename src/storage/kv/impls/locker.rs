//! KV trait impls for the locker table.

use diesel::{BoolExpressionMethods, ExpressionMethods, QueryDsl, associations::HasTable};
use diesel_async::RunQueryDsl;

use crate::{
    error::{KvError, VaultDBError},
    storage::{
        Storage,
        kv::{
            StorageScheme,
            entity::EntityType,
            partition_key::{KvStorePartition, PartitionKey},
            resource::{HashKeyed, KvDeletable, KvFind, KvWriteError, StorageResource},
            serializable_query::generate_insert_query,
        },
        schema,
        types::{Locker, LockerInner, LockerKvValue},
    },
};

impl EntityType for LockerKvValue {
    const ENTITY_TYPE: &'static str = "locker";
}

impl KvStorePartition for LockerKvValue {}

impl HashKeyed for LockerKvValue {}

impl StorageResource for LockerKvValue {
    type Domain = Locker;
    type Error = VaultDBError;

    fn into_domain(self) -> Self::Domain {
        self.into()
    }

    fn set_storage_scheme(&mut self, scheme: StorageScheme) {
        self.updated_by = scheme;
    }

    fn insert_drainer_query(
        &self,
    ) -> error_stack::Result<crate::storage::kv::serializable_query::SerializableQuery, KvError>
    {
        generate_insert_query::<schema::locker::table, _>(self.clone())
    }

    async fn storage_insert(
        self,
        store: &Storage,
    ) -> Result<Locker, crate::error::ContainerError<VaultDBError>> {
        let mut conn = store.get_conn().await?;
        let row: LockerInner = diesel::insert_into(LockerInner::table())
            .values(self)
            .get_result(&mut conn)
            .await?;
        Ok(row.into())
    }
}

impl KvFind for LockerKvValue {
    async fn storage_find(
        store: &Storage,
        pk: &PartitionKey<'_>,
    ) -> Result<Locker, crate::error::ContainerError<VaultDBError>> {
        let PartitionKey::Locker {
            locker_id,
            merchant_id,
            customer_id,
        } = pk
        else {
            return Err(VaultDBError::from(&KvError::Backend).into());
        };
        let mut conn = store.get_conn().await?;
        let row: LockerInner = LockerInner::table()
            .filter(
                schema::locker::locker_id
                    .eq(*locker_id)
                    .and(schema::locker::merchant_id.eq(*merchant_id))
                    .and(schema::locker::customer_id.eq(*customer_id)),
            )
            .get_result(&mut conn)
            .await?;
        Ok(row.into())
    }
}

impl KvDeletable for LockerKvValue {
    async fn storage_delete(
        store: &Storage,
        pk: &PartitionKey<'_>,
    ) -> Result<usize, crate::error::ContainerError<VaultDBError>> {
        let PartitionKey::Locker {
            locker_id,
            merchant_id,
            customer_id,
        } = pk
        else {
            return Err(VaultDBError::from(&KvError::Backend).into());
        };
        let mut conn = store.get_conn().await?;
        let rows = diesel::delete(LockerInner::table())
            .filter(
                schema::locker::locker_id
                    .eq(*locker_id)
                    .and(schema::locker::merchant_id.eq(*merchant_id))
                    .and(schema::locker::customer_id.eq(*customer_id)),
            )
            .execute(&mut conn)
            .await?;
        Ok(rows)
    }
}

impl From<&KvError> for VaultDBError {
    fn from(e: &KvError) -> Self {
        match e {
            KvError::DuplicateValue { .. } => Self::Duplicate,
            KvError::ValueNotFound(_) => Self::NotFoundError,
            KvError::Backend | KvError::SerializationFailed => Self::UnknownError,
        }
    }
}

impl From<KvWriteError> for VaultDBError {
    fn from(e: KvWriteError) -> Self {
        match e {
            KvWriteError::Duplicate => Self::Duplicate,
            KvWriteError::Backend(r) => r.current_context().into(),
        }
    }
}
