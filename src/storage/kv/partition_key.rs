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
    ReverseLookup {
        lookup_id: &'a str,
    },
}

impl std::fmt::Display for PartitionKey<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CombinationKey { combination } => f.write_str(combination),
            Self::Fingerprint { fingerprint_hash } => {
                write!(f, "fingerprint_{}", hex::encode(fingerprint_hash))
            }
            Self::Locker {
                merchant_id,
                customer_id,
                locker_id,
            } => write!(f, "locker_{merchant_id}_{customer_id}_{locker_id}"),
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
}
