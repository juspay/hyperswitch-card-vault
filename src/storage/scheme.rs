//! The [`StorageScheme`] enum — which storage backend was used for a row.
//!
//! This module is **always compiled** (not gated by the `kv` feature) because
//! `updated_by: StorageScheme` appears in every row struct.  The KV-specific
//! decision logic (`decide_storage_scheme`, `TableKvSettings`, `Op`) lives in
//! [`super::kv::scheme`].
//!
//! Diesel mapping: the column is `VARCHAR` (a diesel alias for `Text`), and the
//! enum serializes via manual `Display` → `"postgres_only"` / `"redis_kv"`.
//! Deserialization is via manual `FromStr`; unknown values are rejected
//! (defensive — the DB column carries no default so every insert must set it
//! explicitly).

use std::str::FromStr;

use diesel::{
    backend::Backend,
    deserialize::{self, FromSql, Queryable},
    pg::Pg,
    serialize::{self, ToSql},
    sql_types::Text,
};

/// Per-tenant storage scheme.
///
/// Vendored concept from `common_enums::enums::MerchantStorageScheme`.
/// When `PostgresOnly`, all reads/writes go directly to Postgres.
/// When `RedisKv`, writes go to Redis (write-through) and a drainer stream
/// replays them to Postgres asynchronously.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
    diesel::expression::AsExpression,
    serde::Deserialize,
    serde::Serialize,
)]
#[diesel(sql_type = Text)]
#[serde(rename_all = "snake_case")]
pub enum StorageScheme {
    #[default]
    PostgresOnly,
    RedisKv,
}

impl std::fmt::Display for StorageScheme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PostgresOnly => f.write_str("postgres_only"),
            Self::RedisKv => f.write_str("redis_kv"),
        }
    }
}

impl FromStr for StorageScheme {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "postgres_only" => Ok(Self::PostgresOnly),
            "redis_kv" => Ok(Self::RedisKv),
            other => Err(format!("unknown storage scheme: {other}")),
        }
    }
}

// ── Diesel ToSql / FromSql impls ─────────────────────────────────────────────

impl ToSql<Text, Pg> for StorageScheme {
    fn to_sql<'b>(
        &'b self,
        out: &mut serialize::Output<'b, '_, Pg>,
    ) -> serialize::Result {
        let s: &str = match self {
            Self::PostgresOnly => "postgres_only",
            Self::RedisKv => "redis_kv",
        };
        <str as ToSql<Text, Pg>>::to_sql(s, out)
    }
}

impl FromSql<Text, Pg> for StorageScheme {
    fn from_sql(bytes: <Pg as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        let s = <String as FromSql<Text, Pg>>::from_sql(bytes)?;
        s.parse::<Self>()
            .map_err(Box::<dyn std::error::Error + Send + Sync>::from)
    }
}

// Diesel's `Queryable` derive on the row structs (`VaultInner`, `LockerInner`,
// …) requires each field type to implement `FromSqlRow<ST, DB>`, which in turn
// needs `Queryable<ST, DB>`.  We provide it manually here (mirroring the
// `Encrypted` pattern in `types.rs`) so that `StorageScheme` can appear in
// `#[derive(Queryable)]` structs.
impl Queryable<Text, Pg> for StorageScheme {
    type Row = String;
    fn build(row: Self::Row) -> deserialize::Result<Self> {
        row.parse::<Self>()
            .map_err(Box::<dyn std::error::Error + Send + Sync>::from)
    }
}
