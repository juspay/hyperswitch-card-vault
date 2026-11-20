use hyperswitch_masking::{PeekInterface, Secret};

/// Encode a tuple without delimiter ambiguity. Lengths count UTF-8 bytes, so even
/// components containing underscores, colons, or length prefixes remain distinct.
pub(crate) fn encode_composite_key(namespace: &str, components: &[&str]) -> String {
    let mut key = namespace.to_owned();
    for component in components {
        key.push('_');
        key.push_str(&component.len().to_string());
        key.push(':');
        key.push_str(component);
    }
    key
}

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
            } => f.write_str(&encode_composite_key("vault", &[entity_id, vault_id])),
            Self::HashTable { data_hash } => {
                write!(f, "hash_table_{}", hex::encode(data_hash.peek()))
            }
            Self::Locker {
                merchant_id,
                customer_id,
            } => f.write_str(&encode_composite_key("locker", &[merchant_id, customer_id])),
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

    use super::{KvStorePartition, PartitionKey, encode_composite_key};
    use crate::config::KvConfig;

    struct TestResource;

    impl KvStorePartition for TestResource {}

    #[test]
    fn locker_partition_keys_do_not_collide() {
        let first = PartitionKey::Locker {
            merchant_id: "a_b",
            customer_id: "c",
        };
        let second = PartitionKey::Locker {
            merchant_id: "a",
            customer_id: "b_c",
        };

        assert_eq!(first.to_string(), "locker_3:a_b_1:c");
        assert_eq!(second.to_string(), "locker_1:a_3:b_c");
        // All keys share a shard here: isolation must come from the key encoding.
        assert_ne!(
            TestResource::data_key(first, 1),
            TestResource::data_key(second, 1)
        );
    }

    #[test]
    fn vault_partition_keys_do_not_collide() {
        let first = PartitionKey::Vault {
            entity_id: "a_b",
            vault_id: "c",
        };
        let second = PartitionKey::Vault {
            entity_id: "a",
            vault_id: "b_c",
        };

        assert_eq!(first.to_string(), "vault_3:a_b_1:c");
        assert_eq!(second.to_string(), "vault_1:a_3:b_c");
        assert_ne!(
            TestResource::data_key(first, 1),
            TestResource::data_key(second, 1)
        );
    }

    #[test]
    fn composite_keys_preserve_component_boundaries() {
        let components = ["", "_", ":", "a", "a_b", "b_c", "1:a", "é", "🔒"];
        let mut keys = std::collections::HashSet::new();

        for first in components {
            for second in components {
                for third in components {
                    assert!(keys.insert(encode_composite_key("test", &[first, second, third])));
                }
            }
        }
        assert_eq!(encode_composite_key("test", &["é", "🔒"]), "test_2:é_4:🔒");
    }

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
        for key in [
            PartitionKey::Locker {
                merchant_id: "merchant_{123}",
                customer_id: "customer_é",
            },
            PartitionKey::Vault {
                entity_id: "entity_{123}",
                vault_id: "vault_é",
            },
        ] {
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
}
