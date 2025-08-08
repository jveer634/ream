use std::{
    num::NonZeroUsize,
    sync::{Arc, RwLock},
};

use lru::LruCache;

pub type SharedCache = Arc<RwLock<LruCache<String, String>>>;
pub const CACHE_SIZE: usize = 100;

pub fn new_cache_service() -> SharedCache {
    Arc::new(RwLock::new(LruCache::new(
        NonZeroUsize::new(CACHE_SIZE).unwrap(),
    )))
}
