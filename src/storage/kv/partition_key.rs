/// The partition key used to route a KV entry to a Redis hash slot (shard) and
/// to derive the drainer stream name.
///
/// Each variant corresponds to one of card-vault's tables.  The key formats
/// follow the kv-data-access-patterns CSV:
///
/// | Table        | Partition key format                                |
/// |--------------|-----------------------------------------------------|
/// | fingerprint  | `fingerprint_{fingerprint_hash_hex}`               |
/// | hash_table   | `hash_{hash_id}` (primary-key addressed)            |
/// | locker       | `locker_{merchant_id}_{customer_id}_{locker_id}`   |
/// | vault        | `vault_{entity_id}_{vault_id}`                      |
///
/// `fingerprint` is content-addressed (its PK *is* the fingerprint hash).
/// `hash_table` is keyed by its surrogate PK `hash_id`; the `data_hash` column
/// is a non-PK lookup, so `find_by_data_hash` routes to Postgres.
/// `locker` and `vault` are keyed by composite primary keys and use Redis hash
/// fields (HSETNX/HGET/HSET).
#[derive(Clone, Debug)]
pub(crate) enum PartitionKey<'a> {
    Fingerprint {
        fingerprint_hash: &'a [u8],
    },
    Hash {
        hash_id: &'a str,
    },
    Locker {
        merchant_id: &'a str,
        customer_id: &'a str,
        locker_id: &'a str,
    },
    Vault {
        entity_id: &'a str,
        vault_id: &'a str,
    },
}

impl std::fmt::Display for PartitionKey<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fingerprint {
                fingerprint_hash,
            } => f.write_str(&format!(
                "fingerprint_{}",
                hex::encode(fingerprint_hash)
            )),
            Self::Hash { hash_id } => f.write_str(&format!("hash_{hash_id}")),
            Self::Locker {
                merchant_id,
                customer_id,
                locker_id,
            } => f.write_str(&format!(
                "locker_{merchant_id}_{customer_id}_{locker_id}"
            )),
            Self::Vault {
                entity_id,
                vault_id,
            } => f.write_str(&format!("vault_{entity_id}_{vault_id}")),
        }
    }
}

/// Derive the Redis hash field name for a composite-keyed partition key.
///
/// `fingerprint` and `hash_table` use plain Redis keys, so they have no hash
/// field.  `locker` and `vault` store the row under the partition key as the
/// Redis key and `locker_{locker_id}` / `vault_{vault_id}` as the hash field.
///
/// # Panics
///
/// Panics if called with a `Fingerprint` or `Hash` partition key — those
/// variants use plain Redis keys, not hash fields.
#[allow(clippy::panic)]
pub(crate) fn hash_field_key(partition_key: &PartitionKey<'_>) -> String {
    match partition_key {
        PartitionKey::Locker { locker_id, .. } => {
            format!("locker_{locker_id}")
        }
        PartitionKey::Vault { vault_id, .. } => {
            format!("vault_{vault_id}")
        }
        PartitionKey::Fingerprint { .. } | PartitionKey::Hash { .. } => {
            panic!("hash_field_key is only defined for Locker and Vault partition keys")
        }
    }
}

/// Trait for types that participate in KV sharding.
///
/// The shard key is derived by CRC32-hashing the `PartitionKey` string and
/// taking the modulo with the number of drainer partitions.
pub(crate) trait KvStorePartition {
    fn partition_number(key: PartitionKey<'_>, num_partitions: u8) -> u32 {
        crc32fast::hash(key.to_string().as_bytes()) % u32::from(num_partitions)
    }

    fn shard_key(key: PartitionKey<'_>, num_partitions: u8) -> String {
        format!("shard_{}", Self::partition_number(key, num_partitions))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn partition_key_display_fingerprint() {
        let hash = [0xde, 0xad, 0xbe, 0xef];
        let key = PartitionKey::Fingerprint {
            fingerprint_hash: &hash,
        };
        assert_eq!(key.to_string(), "fingerprint_deadbeef");
    }

    #[test]
    fn partition_key_display_hash() {
        let key = PartitionKey::Hash {
            hash_id: "abc123",
        };
        assert_eq!(key.to_string(), "hash_abc123");
    }

    #[test]
    fn partition_key_display_locker() {
        let key = PartitionKey::Locker {
            merchant_id: "merchant_123",
            customer_id: "cust_abc",
            locker_id: "card_ref_123",
        };
        assert_eq!(
            key.to_string(),
            "locker_merchant_123_cust_abc_card_ref_123"
        );
    }

    #[test]
    fn partition_key_display_vault() {
        let key = PartitionKey::Vault {
            entity_id: "merchant_123",
            vault_id: "vault_456",
        };
        assert_eq!(key.to_string(), "vault_merchant_123_vault_456");
    }

    #[test]
    fn hash_field_key_for_locker() {
        let key = PartitionKey::Locker {
            merchant_id: "m",
            customer_id: "c",
            locker_id: "card_ref_123",
        };
        assert_eq!(hash_field_key(&key), "locker_card_ref_123");
    }

    #[test]
    fn hash_field_key_for_vault() {
        let key = PartitionKey::Vault {
            entity_id: "m",
            vault_id: "vault_456",
        };
        assert_eq!(hash_field_key(&key), "vault_vault_456");
    }

    #[test]
    fn shard_key_is_stable_and_partitioned() {
        struct Dummy;
        impl KvStorePartition for Dummy {}

        let key = PartitionKey::Hash {
            hash_id: "test_hash_id",
        };
        let num_partitions: u8 = 16;
        let shard = Dummy::shard_key(key, num_partitions);
        assert!(shard.starts_with("shard_"));
        let n: u32 = shard
            .strip_prefix("shard_")
            .unwrap()
            .parse()
            .unwrap();
        assert!(n < u32::from(num_partitions));

        let key2 = PartitionKey::Hash {
            hash_id: "test_hash_id",
        };
        assert_eq!(Dummy::shard_key(key2, num_partitions), shard);
    }
}
