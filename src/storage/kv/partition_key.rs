use hyperswitch_masking::{PeekInterface, Secret};

/// Partition key for Redis data-key routing and drainer stream derivation.
#[derive(Clone, Debug)]
pub(crate) enum PartitionKey<'a> {
    CombinationKey {
        combination: &'a str,
    },
    Fingerprint {
        fingerprint_hash: &'a Secret<Vec<u8>>,
    },
    HashTable {
        data_hash: &'a Secret<Vec<u8>>,
    },
    Locker {
        merchant_id: &'a str,
        customer_id: &'a str,
    },
    ReverseLookup {
        lookup_id: &'a str,
    },
    Vault {
        entity_id: &'a str,
        vault_id: &'a str,
    },
}

impl std::fmt::Display for PartitionKey<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CombinationKey { combination } => f.write_str(combination),
            Self::Fingerprint { fingerprint_hash } => {
                write!(f, "fingerprint_{}", hex::encode(fingerprint_hash.peek()))
            }
            Self::Vault {
                entity_id,
                vault_id,
            } => write!(f, "vault_{entity_id}_{vault_id}"),
            Self::HashTable { data_hash } => {
                write!(f, "hash_table_{}", hex::encode(data_hash.peek()))
            }
            Self::Locker {
                merchant_id,
                customer_id,
            } => write!(f, "locker_{merchant_id}_{customer_id}"),
            Self::ReverseLookup { lookup_id } => write!(f, "reverse_lookup_{lookup_id}"),
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

    fn data_key(key: PartitionKey<'_>, num_partitions: u8) -> String {
        let partition_key = key.to_string();
        let shard_key = Self::shard_key(key, num_partitions);
        format!("{{{shard_key}}}:{partition_key}")
    }
}

#[cfg(test)]
mod tests {
    use fred::util::redis_keyslot;

    use super::{KvStorePartition, PartitionKey};
    use crate::config::KvConfig;

    struct TestResource;

    impl KvStorePartition for TestResource {}

    #[test]
    fn data_key_preserves_partition_key_and_adds_hash_tag() {
        let key = PartitionKey::CombinationKey {
            combination: "locker_merchant_customer",
        };
        let shard_key = TestResource::shard_key(key.clone(), 16);

        assert_eq!(
            TestResource::data_key(key, 16),
            format!("{{{shard_key}}}:locker_merchant_customer")
        );
    }

    #[test]
    fn data_key_and_drainer_stream_use_same_redis_cluster_slot() {
        let key = PartitionKey::CombinationKey {
            combination: "locker_merchant_customer",
        };
        let shard_key = TestResource::shard_key(key.clone(), 16);
        let data_key = format!("public:{}", TestResource::data_key(key, 16));
        let stream_name = format!(
            "public:{}",
            KvConfig::default().drainer_stream_name(&shard_key)
        );

        assert_eq!(
            redis_keyslot(data_key.as_bytes()),
            redis_keyslot(stream_name.as_bytes())
        );
    }
}
