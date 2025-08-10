use std::{
    num::NonZeroUsize,
    sync::{Arc, RwLock},
};

use lru::LruCache;

pub type SharedCache = RwLock<LruCache<String, String>>;
pub const CACHE_SIZE: usize = 100;

pub fn new_cache_service() -> SharedCache {
    RwLock::new(LruCache::new(NonZeroUsize::new(CACHE_SIZE).unwrap()))
}
