use std::sync::Arc;

use super::types;

pub(super) type Cache<T, U> =
    moka::future::Cache<<T as super::Cacheable<U>>::Key, Arc<<T as super::Cacheable<U>>::Value>>;

#[derive(Clone)]
pub struct Caching<T>
where
    T: super::Cacheable<types::Merchant>
        + super::Cacheable<types::HashTable>
        + super::Cacheable<types::Fingerprint>,
{
    inner: T,
    merchant_cache: Cache<T, types::Merchant>,
    hash_table_cache: Cache<T, types::HashTable>,
    fingerprint_cache: Cache<T, types::Fingerprint>,
}

impl<T> std::ops::Deref for Caching<T>
where
    T: super::Cacheable<types::Merchant>
        + super::Cacheable<types::HashTable>
        + super::Cacheable<types::Fingerprint>,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

fn new_cache<T, U>(config: &crate::config::Cache, name: &str) -> Cache<T, U>
where
    T: super::Cacheable<U>,
{
    let cache = moka::future::CacheBuilder::new(config.max_capacity).name(name);
    let cache = match config.tti {
        Some(value) => cache.time_to_idle(std::time::Duration::from_secs(value)),
        None => cache,
    };

    cache.build()
}

pub trait GetCache<T, U>
where
    T: super::Cacheable<U>,
{
    fn get_cache(&self) -> &Cache<T, U>;
}

impl<T> GetCache<T, types::Merchant> for Caching<T>
where
    T: super::Cacheable<types::Merchant>
        + super::Cacheable<types::HashTable>
        + super::Cacheable<types::Fingerprint>,
{
    fn get_cache(&self) -> &Cache<T, types::Merchant> {
        &self.merchant_cache
    }
}

impl<T> GetCache<T, types::HashTable> for Caching<T>
where
    T: super::Cacheable<types::Merchant>
        + super::Cacheable<types::HashTable>
        + super::Cacheable<types::Fingerprint>,
{
    fn get_cache(&self) -> &Cache<T, types::HashTable> {
        &self.hash_table_cache
    }
}

impl<T> GetCache<T, types::Fingerprint> for Caching<T>
where
    T: super::Cacheable<types::Merchant>
        + super::Cacheable<types::HashTable>
        + super::Cacheable<types::Fingerprint>,
{
    fn get_cache(&self) -> &Cache<T, types::Fingerprint> {
        &self.fingerprint_cache
    }
}

impl<T> Caching<T>
where
    T: super::Cacheable<types::Merchant>
        + super::Cacheable<types::HashTable>
        + super::Cacheable<types::Fingerprint>,
{
    #[inline(always)]
    pub async fn lookup<U>(
        &self,
        key: &<T as super::Cacheable<U>>::Key,
    ) -> Option<<T as super::Cacheable<U>>::Value>
    where
        T: super::Cacheable<U>,
        Self: GetCache<T, U>,
    {
        self.get_cache()
            .get(key)
            .await
            .map(|value: Arc<<T as super::Cacheable<U>>::Value>| {
                let data = value.as_ref();
                data.clone()
            })
    }

    #[inline(always)]
    pub async fn cache_data<U>(
        &self,
        key: <T as super::Cacheable<U>>::Key,
        value: <T as super::Cacheable<U>>::Value,
    ) where
        T: super::Cacheable<U>,
        Self: GetCache<T, U>,
    {
        self.get_cache().insert(key, value.into()).await;
    }

    pub fn implement_cache(config: &'_ crate::config::Cache) -> impl Fn(T) -> Self + '_ {
        move |inner: T| {
            let merchant_cache = new_cache::<T, types::Merchant>(config, "merchant");
            let hash_table_cache = new_cache::<T, types::HashTable>(config, "hash_table");
            let fingerprint_cache = new_cache::<T, types::Fingerprint>(config, "fingerprint");
            Self {
                inner,
                merchant_cache,
                hash_table_cache,
                fingerprint_cache,
            }
        }
    }
}

pub mod fingerprint;
pub mod hash_table;
pub mod merchant;
