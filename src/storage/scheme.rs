//! The [`StorageScheme`] enum — which storage backend wrote a row.
//! Always compiled (not kv-gated): `updated_by` appears in every row struct.
//! KV decision logic lives in [`super::kv::scheme`].

use diesel::{
    backend::Backend,
    deserialize::{self, FromSql},
    pg::Pg,
    serialize::{self, ToSql},
    sql_types::Text,
};

/// Per-tenant storage scheme.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
    diesel::expression::AsExpression,
    diesel::deserialize::FromSqlRow,
    serde::Deserialize,
    serde::Serialize,
    strum::Display,
    strum::EnumString,
    strum::AsRefStr,
)]
#[diesel(sql_type = Text)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum StorageScheme {
    #[default]
    PostgresOnly,
    RedisKv,
}

impl ToSql<Text, Pg> for StorageScheme {
    fn to_sql<'b>(&'b self, out: &mut serialize::Output<'b, '_, Pg>) -> serialize::Result {
        <str as ToSql<Text, Pg>>::to_sql(self.as_ref(), out)
    }
}

impl FromSql<Text, Pg> for StorageScheme {
    fn from_sql(bytes: <Pg as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        let s = <String as FromSql<Text, Pg>>::from_sql(bytes)?;
        s.parse::<Self>()
            .map_err(Box::<dyn std::error::Error + Send + Sync>::from)
    }
}
