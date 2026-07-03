/// Partition key for Redis hash-slot routing and drainer stream derivation.
#[derive(Clone, Debug)]
pub(crate) enum PartitionKey<'a> {
    Fingerprint {
        fingerprint_hash: &'a [u8],
    },
    Hash {
        data_hash: &'a [u8],
    },
    Locker {
        locker_id: &'a str,
        merchant_id: &'a str,
        customer_id: &'a str,
    },
    Vault {
        vault_id: &'a str,
        entity_id: &'a str,
    },
}

impl std::fmt::Display for PartitionKey<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fingerprint { fingerprint_hash } => {
                f.write_str(&format!("fingerprint_{}", hex::encode(fingerprint_hash)))
            }
            Self::Hash { data_hash } => f.write_str(&format!("hash_{}", hex::encode(data_hash))),
            Self::Locker {
                locker_id,
                merchant_id,
                customer_id,
            } => f.write_str(&format!("locker_{merchant_id}_{customer_id}_{locker_id}")),
            Self::Vault {
                vault_id,
                entity_id,
            } => f.write_str(&format!("vault_{entity_id}_{vault_id}")),
        }
    }
}

/// Redis hash field name for a hash-keyed partition key (locker, vault).
///
/// Returns the field used in `HGet`/`HSetNx`/`Hset` for composite-keyed tables.
/// For plain-keyed tables (`Fingerprint`, `Hash`) the field is the entity type
/// constant (`M::ENTITY_TYPE`), so this helper is not called for them.
pub(crate) fn hash_field_key(partition_key: &PartitionKey<'_>) -> String {
    match partition_key {
        PartitionKey::Locker { locker_id, .. } => format!("locker_{locker_id}"),
        PartitionKey::Vault { vault_id, .. } => format!("vault_{vault_id}"),
        // Fingerprint and Hash are plain-keyed; they use M::ENTITY_TYPE as the field.
        // A mismatch is a programming error, not a runtime condition — return a
        // stable sentinel rather than panicking in library code.
        PartitionKey::Fingerprint { .. } | PartitionKey::Hash { .. } => {
            "invalid_hash_field_key".to_string()
        }
    }
}

/// Types that participate in KV sharding.
pub(crate) trait KvStorePartition {
    fn partition_number(key: PartitionKey<'_>, num_partitions: u8) -> u32 {
        crc32fast::hash(key.to_string().as_bytes()) % u32::from(num_partitions)
    }

    fn shard_key(key: PartitionKey<'_>, num_partitions: u8) -> String {
        format!("shard_{}", Self::partition_number(key, num_partitions))
    }
}
