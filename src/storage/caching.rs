use std::sync::Arc;

#[derive(Clone)]
pub struct Caching<T, U>
where
    T: super::Cacheable<U>,
{
    inner: T,
    cache: moka::future::Cache<T::Key, Arc<T::Value>>,
}

// impl<U, T: super::Cacheable<U>> super::Cacheable<U> for Caching<T, U> {
//     type Key = T::Key;
//     type Value = T::Value;
// }

impl<U1, U2, T> super::Cacheable<U2> for Caching<T, U1>
where
    T: super::Cacheable<U2> + super::Cacheable<U1>,
{
    type Key = <T as super::Cacheable<U2>>::Key;

    type Value = <T as super::Cacheable<U2>>::Value;
}

impl<T: super::Cacheable<U>, U> std::ops::Deref for Caching<T, U> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T, U> Caching<T, U>
where
    T: super::Cacheable<U>,
{
    #[inline(always)]
    pub async fn lookup(&self, key: T::Key) -> Option<T::Value> {
        self.cache.get(&key).await.map(|value| {
            let data = value.as_ref();
            data.clone()
        })
    }

    #[inline(always)]
    pub async fn cache_data(&self, key: T::Key, value: T::Value) {
        self.cache.insert(key, value.into()).await;
    }
}

pub fn implement_cache<'a, T, U>(
    name: &'a str,
    config: &'a crate::config::Cache,
) -> impl Fn(T) -> Caching<T, U> + 'a
where
    T: super::Cacheable<U>,
{
    // Caching { inner, cache }
    move |inner| {
        let cache = moka::future::CacheBuilder::new(config.max_capacity).name(name);
        let cache = match config.tti {
            Some(value) => cache.time_to_idle(std::time::Duration::from_secs(value)),
            None => cache,
        };

        Caching {
            inner,
            cache: cache.build(),
        }
    }
}

pub mod hash_table;
pub mod merchant;
