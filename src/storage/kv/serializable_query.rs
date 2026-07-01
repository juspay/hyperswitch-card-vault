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

    pub fn serialize<S: Serializer>(metadata: &[PgTypeMetadata], s: S) -> Result<S::Ok, S::Error> {
        let pairs: Vec<(u32, u32)> = metadata
            .iter()
            .map(|m| (m.oid().unwrap_or(0), m.array_oid().unwrap_or(0)))
            .collect();
        pairs.serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<PgTypeMetadata>, D::Error> {
        let pairs: Vec<(u32, u32)> = Vec::deserialize(d)?;
        Ok(pairs
            .into_iter()
            .map(|(oid, array_oid)| PgTypeMetadata::from_result(Ok((oid, array_oid))))
            .collect())
    }
}

use diesel::{
    Insertable,
    associations::HasTable,
    debug_query,
    pg::Pg,
    query_builder::{
        InsertStatement, QueryBuilder, QueryFragment, bind_collector::RawBytesBindCollector,
    },
    query_source::Table,
};
use error_stack::ResultExt;
use hyperswitch_masking::Secret;
use tracing::debug;

use super::entity::EntityType;
use crate::error::KvError;

type SecretBinaryData = Secret<Vec<u8>>;

/// No-op `PgMetadataLookup` that issues fake OIDs for custom types.
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

/// SQL query and bind parameters in a serializable representation.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct SerializableQuery {
    sql: String,

    #[serde(with = "bind_params")]
    binds: Vec<Option<SecretBinaryData>>,

    #[serde(with = "pg_type_metadata")]
    metadata: Vec<diesel::pg::PgTypeMetadata>,

    safe_to_cache_prepared: bool,

    entity_type: String,

    operation: DatabaseOperation,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, strum::Display,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub(crate) enum DatabaseOperation {
    Insert,
}

impl SerializableQuery {
    pub(crate) fn entity_type(&self) -> String {
        self.entity_type.clone()
    }

    pub(crate) fn operation(&self) -> DatabaseOperation {
        self.operation
    }

    /// Construct a `SerializableQuery` from any diesel query fragment.
    fn from_query<Q>(
        query: Q,
        entity_type: String,
        operation: DatabaseOperation,
    ) -> error_stack::Result<Self, KvError>
    where
        Q: QueryFragment<Pg> + Send + 'static,
    {
        debug!(%entity_type, %operation, query = %debug_query::<Pg, _>(&query).to_string());

        let mut qb = diesel::pg::PgQueryBuilder::new();
        query
            .to_sql(&mut qb, &Pg)
            .change_context(KvError::SerializationFailed)
            .attach_printable("Failed to construct SQL query")?;
        let sql = qb.finish();

        let safe_to_cache_prepared = query
            .is_safe_to_cache_prepared(&Pg)
            .change_context(KvError::SerializationFailed)
            .attach_printable(
                "Failed to determine whether query is safe to store in prepared statement cache",
            )?;

        let mut bind_collector = RawBytesBindCollector::<Pg>::new();
        let mut metadata_lookup = KvPgMetadataLookup;
        query
            .collect_binds(&mut bind_collector, &mut metadata_lookup, &Pg)
            .change_context(KvError::SerializationFailed)
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

    pub(crate) fn to_field_value_pairs(
        &self,
        request_id: String,
        global_id: String,
    ) -> error_stack::Result<Vec<(&str, String)>, KvError> {
        let pushed_at = time::OffsetDateTime::now_utc().unix_timestamp();

        Ok(vec![
            (
                "query",
                serde_json::to_string(self).change_context(KvError::SerializationFailed)?,
            ),
            ("global_id", global_id),
            ("request_id", request_id),
            ("pushed_at", pushed_at.to_string()),
        ])
    }
}

pub(crate) fn generate_insert_query<T, N>(
    new: N,
) -> error_stack::Result<SerializableQuery, KvError>
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
