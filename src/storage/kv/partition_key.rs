/// The partition key used to route a KV entry to a Redis hash slot (shard) and
/// to derive the drainer stream name.
///
/// Each variant corresponds to one of card-vault's tables.  The key formats
/// follow the kv-data-access-patterns CSV:
///
/// | Table        | Partition key format                                |
/// |--------------|-----------------------------------------------------|
/// | fingerprint  | `fingerprint_{fingerprint_hash_hex}`               |
/// | hash_table   | `hash_{data_hash_hex}` (content-addressed)         |
/// | locker       | `locker_{merchant_id}_{customer_id}_{locker_id}`   |
/// | vault        | `vault_{entity_id}_{vault_id}`                      |
///
/// `fingerprint` and `hash_table` are content-addressed (found by their hash),
/// so no reverse lookup is needed.  `locker` and `vault` variants are defined
/// for reference per the CSV map but not wired in this phase.
#[derive(Clone)]
pub enum PartitionKey<'a> {
    /// Partition key for the `fingerprint` table.
    /// `find_by_fingerprint_hash` and `get_or_insert_fingerprint` both use this key.
    Fingerprint {
        fingerprint_hash: &'a [u8],
    },
    /// Partition key for the `hash_table` table (v1).  Content-addressed by
    /// `data_hash` (the only read path); `hash_id` is a surrogate consumed by
    /// locker.  Not needing a reverse lookup.
    Hash {
        data_hash: &'a [u8],
    },
    /// Partition key for the `locker` table (v1).  Not wired in this phase.
    Locker {
        merchant_id: &'a str,
        customer_id: &'a str,
        locker_id: &'a str,
    },
    /// Partition key for the `vault` table (v2).  Not wired in this phase.
    Vault {
        entity_id: &'a str,
        vault_id: &'a str,
    },
    /// A free-form combination key, used for reverse lookups.
    CombinationKey {
        combination: &'a str,
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
            Self::Hash { data_hash } => f.write_str(&format!("hash_{}", hex::encode(data_hash))),
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
            Self::CombinationKey { combination } => f.write_str(combination),
        }
    }
}

/// Trait for types that participate in KV sharding.
///
/// The shard key is derived by CRC32-hashing the `PartitionKey` string and
/// taking the modulo with the number of drainer partitions.
pub trait KvStorePartition {
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
        let hash = [0xab, 0xcd, 0xef, 0x01];
        let key = PartitionKey::Hash {
            data_hash: &hash,
        };
        assert_eq!(key.to_string(), "hash_abcdef01");
    }

    #[test]
    fn partition_key_display_locker() {
        let key = PartitionKey::Locker {
            merchant_id: "merchant_1",
            customer_id: "cust_1",
            locker_id: "locker_1",
        };
        assert_eq!(
            key.to_string(),
            "locker_merchant_1_cust_1_locker_1"
        );
    }

    #[test]
    fn partition_key_display_vault() {
        let key = PartitionKey::Vault {
            entity_id: "ent_123",
            vault_id: "vlt_456",
        };
        assert_eq!(key.to_string(), "vault_ent_123_vlt_456");
    }

    #[test]
    fn partition_key_display_combination() {
        let key = PartitionKey::CombinationKey {
            combination: "custom_key",
        };
        assert_eq!(key.to_string(), "custom_key");
    }

    #[test]
    fn shard_key_is_stable_and_partitioned() {
        struct Dummy;
        impl KvStorePartition for Dummy {}

        let hash = [0u8; 4];
        let key = PartitionKey::Hash {
            data_hash: &hash,
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
            data_hash: &hash,
        };
        assert_eq!(Dummy::shard_key(key2, num_partitions), shard);
    }
}
