/// Partition key for Redis hash-slot routing and drainer stream derivation.
#[derive(Clone, Debug)]
pub(crate) enum PartitionKey<'a> {
    CombinationKey {
        combination: &'a str,
    },
    Fingerprint {
        fingerprint_hash: &'a [u8],
    },
    Locker {
        merchant_id: &'a str,
        customer_id: &'a str,
        locker_id: &'a str,
    },
    HashTable {
        data_hash: &'a [u8],
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
            Self::CombinationKey { combination } => f.write_str(&format!("{}", combination)),
            Self::Fingerprint { fingerprint_hash } => {
                f.write_str(&format!("fingerprint_{}", hex::encode(fingerprint_hash)))
            }
            Self::Locker {
                merchant_id,
                customer_id,
                locker_id,
            } => f.write_str(&format!("locker_{merchant_id}_{customer_id}_{locker_id}")),
            Self::HashTable { data_hash } => {
                f.write_str(&format!("hash_table_{}", hex::encode(data_hash)))
            }
            Self::ReverseLookup { lookup_id } => {
                f.write_str(&format!("reverse_lookup_{lookup_id}"))
            }
            Self::Vault {
                entity_id,
                vault_id,
            } => f.write_str(&format!("vault_{entity_id}_{vault_id}")),
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
