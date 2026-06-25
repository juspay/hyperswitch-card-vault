//! The [`StorageScheme`] enum — which storage backend was used for a row.
//!
//! This module is **always compiled** (not gated by the `kv` feature) because
//! `updated_by: StorageScheme` appears in every row struct.  The KV-specific
//! decision logic (`decide_storage_scheme`, `TableKvSettings`, `Op`) lives in
//! [`super::kv::scheme`].
//!
//! Diesel mapping: the column is `VARCHAR` (a diesel alias for `Text`), and the
//! enum serializes via `strum::Display` → `"postgres_only"` / `"redis_kv"`.
//! Deserialization is via `strum::EnumString`; unknown values are rejected
//! (defensive — the DB column carries no default so every insert must set it
//! explicitly).

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
    strum::Display,
    strum::EnumString,
)]
#[diesel(sql_type = Text)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum StorageScheme {
    #[default]
    PostgresOnly,
    RedisKv,
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn display_snake_case() {
        assert_eq!(StorageScheme::PostgresOnly.to_string(), "postgres_only");
        assert_eq!(StorageScheme::RedisKv.to_string(), "redis_kv");
    }

    #[test]
    fn parse_round_trip() {
        for variant in [StorageScheme::PostgresOnly, StorageScheme::RedisKv] {
            let s = variant.to_string();
            let parsed: StorageScheme = s.parse().unwrap();
            assert_eq!(variant, parsed);
        }
    }

    #[test]
    fn from_sql_rejects_unknown_value() {
        // FromSql delegates to strum::EnumString's parse — verify that
        // an unknown string is rejected rather than silently defaulting.
        let result = "mysql_kv".parse::<StorageScheme>();
        assert!(result.is_err());
    }

    #[test]
    fn serde_round_trip() {
        for variant in [StorageScheme::PostgresOnly, StorageScheme::RedisKv] {
            let json = serde_json::to_string(&variant).unwrap();
            let parsed: StorageScheme = serde_json::from_str(&json).unwrap();
            assert_eq!(variant, parsed);
        }
    }
}
