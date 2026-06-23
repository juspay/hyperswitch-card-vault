mod bind_params {
    use base64::Engine;
    use hyperswitch_masking::{PeekInterface, Secret};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use super::SecretBinaryData;

    pub fn serialize<S: Serializer>(
        binds: &[Option<SecretBinaryData>],
        s: S,
    ) -> Result<S::Ok, S::Error> {
        let encoded: Vec<Option<String>> = binds
            .iter()
            .map(|b| {
                b.as_ref()
                    .map(|bytes| base64::engine::general_purpose::STANDARD.encode(bytes.peek()))
            })
            .collect();
        encoded.serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<Vec<Option<SecretBinaryData>>, D::Error> {
        let encoded: Vec<Option<String>> = Vec::deserialize(d)?;
        encoded
            .into_iter()
            .map(|b| {
                b.map(|s| {
                    base64::engine::general_purpose::STANDARD
                        .decode(&s)
                        .map(Secret::new)
                        .map_err(serde::de::Error::custom)
                })
                .transpose()
            })
            .collect()
    }
}

mod pg_type_metadata {
    use diesel::pg::PgTypeMetadata;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(
        metadata: &[PgTypeMetadata],
        s: S,
    ) -> Result<S::Ok, S::Error> {
        let pairs: Vec<(u32, u32)> = metadata
            .iter()
            .map(|m| (m.oid().unwrap_or(0), m.array_oid().unwrap_or(0)))
            .collect();
        pairs.serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<Vec<PgTypeMetadata>, D::Error> {
        let pairs: Vec<(u32, u32)> = Vec::deserialize(d)?;
        Ok(pairs
            .into_iter()
            .map(|(oid, array_oid)| PgTypeMetadata::from_result(Ok((oid, array_oid))))
            .collect())
    }
}

use std::fmt::Debug;

use diesel::{
    associations::HasTable,
    debug_query,
    dsl::{Filter, Find},
    pg::Pg,
    query_builder::{
        bind_collector::RawBytesBindCollector, AsChangeset, AsQuery, CollectedQuery,
        InsertStatement, IntoUpdateTarget, MoveableBindCollector, QueryBuilder, QueryFragment,
        UpdateStatement,
    },
    query_dsl::methods::{FilterDsl, FindDsl},
    query_source::Table,
    Insertable,
};
use diesel_async::RunQueryDsl;
use error_stack::ResultExt;
use hyperswitch_masking::{ExposeInterface, Secret};
use tracing::debug;

use super::entity::EntityType;
use crate::error::StorageError;

type SecretBinaryData = Secret<Vec<u8>>;

/// A no-op [`diesel::pg::PgMetadataLookup`] that issues fake OIDs for custom types.
///
/// This mirrors the approach `diesel-async` uses internally in its
/// `construct_bind_data` function: binds are collected synchronously without a
/// live connection, and custom-type OIDs are resolved later (by the drainer when
/// it replays the query).  For standard built-in types the real OIDs are
/// embedded directly by diesel, so only user-defined types receive fakes.
const FAKE_OID: u32 = 0;

struct KvPgMetadataLookup;

impl diesel::pg::PgMetadataLookup for KvPgMetadataLookup {
    fn lookup_type(
        &mut self,
        _type_name: &str,
        _schema: Option<&str>,
    ) -> diesel::pg::PgTypeMetadata {
        diesel::pg::PgTypeMetadata::from_result(Ok((FAKE_OID, FAKE_OID)))
    }
}

/// The SQL query and its bind parameters, in a (de)serialization-friendly representation
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct SerializableQuery {
    /// The SQL query
    sql: String,

    /// The serialized bytes for each bind parameter
    #[serde(with = "bind_params")]
    binds: Vec<Option<SecretBinaryData>>,

    /// The metadata associated with each bind parameter
    #[serde(with = "pg_type_metadata")]
    metadata: Vec<diesel::pg::PgTypeMetadata>,

    /// Whether this query is safe to store in the prepared statement cache
    safe_to_cache_prepared: bool,

    /// Entity type
    entity_type: String,

    /// The type of database operation
    operation: DatabaseOperation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, strum::Display)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum DatabaseOperation {
    Insert,
    Update,
}

impl SerializableQuery {
    pub fn entity_type(&self) -> String {
        self.entity_type.clone()
    }

    pub fn operation(&self) -> DatabaseOperation {
        self.operation
    }

    /// Construct a [`SerializableQuery`] from any diesel query fragment.
    ///
    /// Unlike the hyperswitch original which needed an `async-bb8-diesel`
    /// connection to collect binds, this version uses a [`KvPgMetadataLookup`]
    /// with fake OIDs, so **no database connection is required**.
    /// `to_sql` and `is_safe_to_cache_prepared` are also connection-free;
    /// only bind collection needed a `PgMetadataLookup`, and we provide a
    /// stand-in.  This is the "Risk 1" adaptation described in the plan.
    fn from_query<Q>(
        query: Q,
        entity_type: String,
        operation: DatabaseOperation,
    ) -> error_stack::Result<Self, StorageError>
    where
        Q: QueryFragment<Pg> + Send + 'static,
    {
        debug!(%entity_type, %operation, query = %debug_query::<Pg, _>(&query).to_string());

        let mut qb = diesel::pg::PgQueryBuilder::new();
        query
            .to_sql(&mut qb, &Pg)
            .change_context(StorageError::SerializationFailed)
            .attach_printable("Failed to construct SQL query")?;
        let sql = qb.finish();

        let safe_to_cache_prepared = query
            .is_safe_to_cache_prepared(&Pg)
            .change_context(StorageError::SerializationFailed)
            .attach_printable(
                "Failed to determine whether query is safe to store in prepared statement cache",
            )?;

        let mut bind_collector = RawBytesBindCollector::<Pg>::new();
        let mut metadata_lookup = KvPgMetadataLookup;
        query
            .collect_binds(&mut bind_collector, &mut metadata_lookup, &Pg)
            .change_context(StorageError::SerializationFailed)
            .attach_printable("Failed to construct bind parameters")?;

        let serializable_query = Self {
            sql,
            binds: bind_collector
                .binds
                .into_iter()
                .map(|option| option.map(Secret::new))
                .collect(),
            metadata: bind_collector.metadata.clone(),
            safe_to_cache_prepared,
            entity_type,
            operation,
        };

        Ok(serializable_query)
    }

    fn to_collected_query(&self) -> CollectedQuery<RawBytesBindCollector<Pg>> {
        let mut bind_collector = RawBytesBindCollector::<Pg>::new();
        bind_collector.binds = self
            .binds
            .clone()
            .into_iter()
            .map(|option| option.map(ExposeInterface::expose))
            .collect();
        bind_collector.metadata = self.metadata.clone();

        CollectedQuery::new(
            self.sql.clone(),
            self.safe_to_cache_prepared,
            bind_collector.moveable(),
        )
    }

    pub async fn execute(
        self,
        conn: &mut diesel_async::AsyncPgConnection,
    ) -> error_stack::Result<usize, StorageError> {
        let query = self.to_collected_query();

        debug!(query = %debug_query::<Pg, _>(&query).to_string());

        query
            .execute(conn)
            .await
            .change_context(StorageError::InsertError)
            .attach_printable("Failed to execute drainer query")
    }

    pub fn to_field_value_pairs(
        &self,
        request_id: String,
        global_id: String,
    ) -> error_stack::Result<Vec<(&str, String)>, StorageError> {
        let pushed_at = now_unix_timestamp();

        Ok(vec![
            (
                "query",
                serde_json::to_string(self)
                    .change_context(StorageError::SerializationFailed)?,
            ),
            ("global_id", global_id),
            ("request_id", request_id),
            ("pushed_at", pushed_at.to_string()),
        ])
    }
}

fn now_unix_timestamp() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp()
}

pub(crate) fn generate_insert_query<T, N>(
    new: N,
) -> error_stack::Result<SerializableQuery, StorageError>
where
    T: HasTable<Table = T> + Table + Send + 'static,
    N: Insertable<T> + EntityType,
    <N as Insertable<T>>::Values: QueryFragment<Pg> + Send + 'static,
    InsertStatement<T, <N as Insertable<T>>::Values>: QueryFragment<Pg> + Send,
{
    let entity_type = N::ENTITY_TYPE.to_owned();
    let query = diesel::insert_into(<T as HasTable>::table()).values(new);
    SerializableQuery::from_query(query, entity_type, DatabaseOperation::Insert)
        .attach_printable("Failed to generate insert query")
}

#[allow(clippy::type_complexity)]
#[allow(dead_code)]
pub(crate) fn generate_update_query_by_id<T, V, Pk>(
    id: Pk,
    values: V,
) -> error_stack::Result<SerializableQuery, StorageError>
where
    T: FindDsl<Pk> + HasTable<Table = T> + Table + 'static,
    V: AsChangeset<Target = <Find<T, Pk> as HasTable>::Table> + EntityType,
    Find<T, Pk>: IntoUpdateTarget + 'static,
    UpdateStatement<
        <Find<T, Pk> as HasTable>::Table,
        <Find<T, Pk> as IntoUpdateTarget>::WhereClause,
        <V as AsChangeset>::Changeset,
    >: AsQuery + QueryFragment<Pg> + Send + 'static,
    Pk: Clone,
{
    let entity_type = V::ENTITY_TYPE.to_owned();
    let query = diesel::update(<T as HasTable>::table().find(id)).set(values);
    SerializableQuery::from_query(query, entity_type, DatabaseOperation::Update)
        .attach_printable("Failed to generate update query (with primary key)")
}

#[allow(clippy::type_complexity)]
#[allow(dead_code)]
pub(crate) fn generate_update_query_with_predicate<T, V, P>(
    predicate: P,
    values: V,
) -> error_stack::Result<SerializableQuery, StorageError>
where
    T: FilterDsl<P> + HasTable<Table = T> + Table + 'static,
    V: AsChangeset<Target = <Filter<T, P> as HasTable>::Table> + EntityType,
    Filter<T, P>: IntoUpdateTarget + 'static,
    UpdateStatement<
        <Filter<T, P> as HasTable>::Table,
        <Filter<T, P> as IntoUpdateTarget>::WhereClause,
        <V as AsChangeset>::Changeset,
    >: AsQuery + QueryFragment<Pg> + Send + 'static,
{
    let entity_type = V::ENTITY_TYPE.to_owned();
    let query = diesel::update(<T as HasTable>::table().filter(predicate)).set(values);
    SerializableQuery::from_query(query, entity_type, DatabaseOperation::Update)
        .attach_printable("Failed to generate update query (with predicate)")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use hyperswitch_masking::PeekInterface;

    use crate::storage::schema;

    #[derive(Debug, Insertable)]
    #[diesel(table_name = schema::hash_table)]
    struct TestHashTableNew {
        hash_id: String,
        data_hash: Vec<u8>,
    }

    impl EntityType for TestHashTableNew {
        const ENTITY_TYPE: &'static str = "hash_table";
    }

    #[test]
    fn serializable_query_json_round_trip() {
        let new = TestHashTableNew {
            hash_id: "test_hash_id".to_string(),
            data_hash: vec![1, 2, 3, 4],
        };
        let query =
            generate_insert_query::<schema::hash_table::table, _>(new).unwrap();

        let json = serde_json::to_string(&query).unwrap();
        let deserialized: SerializableQuery = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.entity_type(), "hash_table");
        assert_eq!(deserialized.operation(), DatabaseOperation::Insert);
        assert!(!deserialized.sql.is_empty());
        assert!(deserialized
            .sql
            .to_lowercase()
            .contains("insert into"));
    }

    #[test]
    fn serializable_query_preserves_sql_and_binds() {
        let new = TestHashTableNew {
            hash_id: "abc".to_string(),
            data_hash: vec![0xde, 0xad, 0xbe, 0xef],
        };
        let query =
            generate_insert_query::<schema::hash_table::table, _>(new).unwrap();

        let json = serde_json::to_string(&query).unwrap();
        let deserialized: SerializableQuery = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.sql, query.sql);
        assert_eq!(
            deserialized.safe_to_cache_prepared,
            query.safe_to_cache_prepared
        );

        let original_binds: Vec<Option<Vec<u8>>> = query
            .binds
            .iter()
            .map(|b| b.as_ref().map(|s| s.peek().clone()))
            .collect();
        let round_trip_binds: Vec<Option<Vec<u8>>> = deserialized
            .binds
            .iter()
            .map(|b| b.as_ref().map(|s| s.peek().clone()))
            .collect();
        assert_eq!(original_binds, round_trip_binds);
    }

    #[test]
    fn to_field_value_pairs_includes_required_fields() {
        let new = TestHashTableNew {
            hash_id: "abc".to_string(),
            data_hash: vec![0xde, 0xad, 0xbe, 0xef],
        };
        let query =
            generate_insert_query::<schema::hash_table::table, _>(new).unwrap();

        let pairs = query
            .to_field_value_pairs("req-123".to_string(), "global-456".to_string())
            .unwrap();

        let field_names: Vec<&str> = pairs.iter().map(|(k, _)| *k).collect();
        assert!(field_names.contains(&"query"));
        assert!(field_names.contains(&"global_id"));
        assert!(field_names.contains(&"request_id"));
        assert!(field_names.contains(&"pushed_at"));

        let global_id = pairs
            .iter()
            .find(|(k, _)| *k == "global_id")
            .map(|(_, v)| v.clone())
            .unwrap();
        assert_eq!(global_id, "global-456");

        let request_id = pairs
            .iter()
            .find(|(k, _)| *k == "request_id")
            .map(|(_, v)| v.clone())
            .unwrap();
        assert_eq!(request_id, "req-123");
    }
}
