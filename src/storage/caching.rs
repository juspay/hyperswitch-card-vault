use std::sync::Arc;

use super::types;

pub(super) type Cache<T, U> =
    moka::future::Cache<<T as super::Cacheable<U>>::Key, Arc<<T as super::Cacheable<U>>::Value>>;

#[cfg(feature = "external_key_manager")]
pub trait CacheableWithEntity<T>: super::Cacheable<types::Entity> {}

#[cfg(feature = "external_key_manager")]
impl<T: super::Cacheable<types::Entity>> CacheableWithEntity<T> for T {}

#[cfg(not(feature = "external_key_manager"))]
pub trait CacheableWithEntity<T> {}

#[cfg(not(feature = "external_key_manager"))]
impl<T> CacheableWithEntity<T> for T {}

#[derive(Clone)]
pub struct Caching<T>
where
    T: super::Cacheable<types::Merchant>
        + super::Cacheable<types::HashTable>
        + super::Cacheable<types::Fingerprint>
        + CacheableWithEntity<T>,
{
    inner: T,
    merchant_cache: Cache<T, types::Merchant>,
    hash_table_cache: Cache<T, types::HashTable>,
    fingerprint_cache: Cache<T, types::Fingerprint>,
    #[cfg(feature = "external_key_manager")]
    entity_cache: Cache<T, types::Entity>,
}

impl<T> std::ops::Deref for Caching<T>
where
    T: super::Cacheable<types::Merchant>
        + super::Cacheable<types::HashTable>
        + super::Cacheable<types::Fingerprint>
        + CacheableWithEntity<T>,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

fn new_cache<T, U>(config: &crate::config::Cache, name: &'static str) -> Cache<T, U>
where
    T: super::Cacheable<U>,
{
    let cache = moka::future::CacheBuilder::new(config.max_capacity)
        .name(name)
        .eviction_listener(move |_key, _value, removal_cause| {
            cache_eviction_listener(name, removal_cause);
        });
    let cache = match config.tti {
        Some(value) => cache.time_to_idle(std::time::Duration::from_secs(value)),
        None => cache,
    };

    cache.build()
}

fn cache_eviction_listener(
    cache_name: &'static str,
    removal_cause: moka::notification::RemovalCause,
) {
    use moka::notification::RemovalCause;

    let removal_cause_label = match removal_cause {
        RemovalCause::Expired => "expired",
        RemovalCause::Explicit => "explicit",
        RemovalCause::Replaced => "replaced",
        RemovalCause::Size => "size",
    };

    crate::observability::metrics::CACHE_EVICTION_COUNT.add(
        1,
        crate::metric_attributes!(
            ("cache", cache_name),
            ("removal_cause", removal_cause_label)
        ),
    );
}

pub trait GetCache<T, U>
where
    T: super::Cacheable<U>,
{
    fn get_cache(&self) -> &Cache<T, U>;
    fn cache_name(&self) -> &'static str;
}

impl<T> GetCache<T, types::Merchant> for Caching<T>
where
    T: super::Cacheable<types::Merchant>
        + super::Cacheable<types::HashTable>
        + super::Cacheable<types::Fingerprint>
        + CacheableWithEntity<T>,
{
    fn get_cache(&self) -> &Cache<T, types::Merchant> {
        &self.merchant_cache
    }

    fn cache_name(&self) -> &'static str {
        types::Merchant::CACHE_NAME
    }
}

impl<T> GetCache<T, types::HashTable> for Caching<T>
where
    T: super::Cacheable<types::Merchant>
        + super::Cacheable<types::HashTable>
        + super::Cacheable<types::Fingerprint>
        + CacheableWithEntity<T>,
{
    fn get_cache(&self) -> &Cache<T, types::HashTable> {
        &self.hash_table_cache
    }

    fn cache_name(&self) -> &'static str {
        types::HashTable::CACHE_NAME
    }
}

impl<T> GetCache<T, types::Fingerprint> for Caching<T>
where
    T: super::Cacheable<types::Merchant>
        + super::Cacheable<types::HashTable>
        + super::Cacheable<types::Fingerprint>
        + CacheableWithEntity<T>,
{
    fn get_cache(&self) -> &Cache<T, types::Fingerprint> {
        &self.fingerprint_cache
    }

    fn cache_name(&self) -> &'static str {
        types::Fingerprint::CACHE_NAME
    }
}

#[cfg(feature = "external_key_manager")]
impl<T> GetCache<T, types::Entity> for Caching<T>
where
    T: super::Cacheable<types::Merchant>
        + super::Cacheable<types::HashTable>
        + super::Cacheable<types::Fingerprint>
        + CacheableWithEntity<T>,
{
    fn get_cache(&self) -> &Cache<T, types::Entity> {
        &self.entity_cache
    }

    fn cache_name(&self) -> &'static str {
        types::Entity::CACHE_NAME
    }
}

impl<T> Caching<T>
where
    T: super::Cacheable<types::Merchant>
        + super::Cacheable<types::HashTable>
        + super::Cacheable<types::Fingerprint>
        + CacheableWithEntity<T>,
{
    pub async fn collect_cache_entry_count(&self, tenant_id: &str) {
        macro_rules! collect {
            ($type:ty) => {{
                let cache = <Self as GetCache<T, $type>>::get_cache(self);
                let name = <Self as GetCache<T, $type>>::cache_name(self);

                cache.run_pending_tasks().await;
                crate::observability::metrics::CACHE_ENTRY_COUNT.record(
                    cache.entry_count(),
                    crate::metric_attributes!(("cache", name), ("tenant_id", tenant_id.to_owned())),
                );
            }};
        }

        collect!(types::Merchant);
        collect!(types::HashTable);
        collect!(types::Fingerprint);
        #[cfg(feature = "external_key_manager")]
        collect!(types::Entity);
    }

    #[inline(always)]
    pub async fn lookup<U>(
        &self,
        key: <T as super::Cacheable<U>>::Key,
    ) -> Option<<T as super::Cacheable<U>>::Value>
    where
        T: super::Cacheable<U>,
        Self: GetCache<T, U>,
    {
        let value = self.get_cache().get(&key).await.map(
            |value: Arc<<T as super::Cacheable<U>>::Value>| {
                let data = value.as_ref();
                data.clone()
            },
        );

        crate::observability::metrics::CACHE_LOOKUP_COUNT.add(
            1,
            crate::metric_attributes!(
                ("cache", <Self as GetCache<T, U>>::cache_name(self)),
                ("outcome", if value.is_some() { "hit" } else { "miss" })
            ),
        );

        value
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

        crate::observability::metrics::CACHE_INSERT_COUNT.add(
            1,
            crate::metric_attributes!(("cache", <Self as GetCache<T, U>>::cache_name(self))),
        );
    }

    pub fn implement_cache(config: &'_ crate::config::Cache) -> impl Fn(T) -> Self + '_ {
        move |inner: T| {
            let merchant_cache =
                new_cache::<T, types::Merchant>(config, types::Merchant::CACHE_NAME);
            let hash_table_cache =
                new_cache::<T, types::HashTable>(config, types::HashTable::CACHE_NAME);
            let fingerprint_cache =
                new_cache::<T, types::Fingerprint>(config, types::Fingerprint::CACHE_NAME);
            #[cfg(feature = "external_key_manager")]
            let entity_cache = new_cache::<T, types::Entity>(config, types::Entity::CACHE_NAME);

            Self {
                inner,
                merchant_cache,
                hash_table_cache,
                fingerprint_cache,
                #[cfg(feature = "external_key_manager")]
                entity_cache,
            }
        }
    }
}

#[cfg(feature = "external_key_manager")]
pub mod entity;
pub mod fingerprint;
pub mod hash_table;
pub mod merchant;
