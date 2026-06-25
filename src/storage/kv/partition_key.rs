/// The partition key used to route a KV entry to a Redis hash slot (shard) and
/// to derive the drainer stream name.
///
/// Each variant corresponds to one of card-vault's tables.  The key formats
/// follow the kv-data-access-patterns CSV:
///
/// | Table        | Partition key format                                |
/// |--------------|-----------------------------------------------------|
/// | fingerprint     | `fingerprint_{fingerprint_hash_hex}`               |
/// | hash_table      | `hash_{data_hash_hex}` (content-addressed)         |
/// | reverse_lookup  | `reverse_lookup_{lookup_id}`                        |
///
/// `fingerprint` and `hash_table` are content-addressed (found by their hash).
/// `reverse_lookup` is keyed by its `lookup_id`.  `locker` and `vault` variants
/// are re-added when those tables gain KV support.
#[derive(Clone)]
pub enum PartitionKey<'a> {
    /// Partition key for the `fingerprint` table.
    /// `find_by_fingerprint_hash` and `get_or_insert_fingerprint` both use this key.
    Fingerprint {
        fingerprint_hash: &'a [u8],
    },
    /// Partition key for the `hash_table` table (v1).  Content-addressed by
    /// `data_hash` (the only read path); `hash_id` is a surrogate consumed by
    /// locker.
    Hash {
        data_hash: &'a [u8],
    },
    /// Partition key for the `reverse_lookup` table, keyed by `lookup_id`.
    ReverseLookup {
        lookup_id: &'a str,
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
            Self::ReverseLookup { lookup_id } => {
                f.write_str(&format!("reverse_lookup_{lookup_id}"))
            }
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
    fn partition_key_display_reverse_lookup() {
        let key = PartitionKey::ReverseLookup {
            lookup_id: "lookup_123",
        };
        assert_eq!(key.to_string(), "reverse_lookup_lookup_123");
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
