/// Partition key for Redis hash-slot routing and drainer stream derivation.
#[derive(Clone, Debug)]
pub(crate) enum PartitionKey<'a> {
    Fingerprint { fingerprint_hash: &'a [u8] },
    ReverseLookup { lookup_id: &'a str },
}

impl std::fmt::Display for PartitionKey<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fingerprint { fingerprint_hash } => {
                f.write_str(&format!("fingerprint_{}", hex::encode(fingerprint_hash)))
            }
            Self::ReverseLookup { lookup_id } => {
                f.write_str(&format!("reverse_lookup_{lookup_id}"))
            }
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
