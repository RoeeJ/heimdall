use dashmap::DashMap;
use parking_lot::Mutex;
use std::sync::Arc;

/// A simple object pool for reusing buffers and reducing allocations
pub struct Pool<T> {
    items: Arc<Mutex<Vec<T>>>,
    factory: Arc<dyn Fn() -> T + Send + Sync>,
    reset: Arc<dyn Fn(&mut T) + Send + Sync>,
    max_size: usize,
}

impl<T> Clone for Pool<T> {
    fn clone(&self) -> Self {
        Self {
            items: Arc::clone(&self.items),
            factory: Arc::clone(&self.factory),
            reset: Arc::clone(&self.reset),
            max_size: self.max_size,
        }
    }
}

impl<T> Pool<T> {
    /// Create a new pool with the given factory function and max size
    pub fn new<F, R>(factory: F, reset: R, max_size: usize) -> Self
    where
        F: Fn() -> T + Send + Sync + 'static,
        R: Fn(&mut T) + Send + Sync + 'static,
    {
        Self {
            items: Arc::new(Mutex::new(Vec::with_capacity(max_size))),
            factory: Arc::new(factory),
            reset: Arc::new(reset),
            max_size,
        }
    }

    /// Get an item from the pool or create a new one
    pub fn get(&self) -> PooledItem<T> {
        let item = {
            let mut items = self.items.lock();
            items.pop()
        };

        let item = item.unwrap_or_else(|| (self.factory)());

        PooledItem {
            item: Some(item),
            pool: self.clone(),
        }
    }

    /// Return an item to the pool
    fn put(&self, mut item: T) {
        (self.reset)(&mut item);

        let mut items = self.items.lock();
        if items.len() < self.max_size {
            items.push(item);
        }
    }
}

/// A pooled item that returns itself to the pool when dropped
pub struct PooledItem<T> {
    item: Option<T>,
    pool: Pool<T>,
}

impl<T> std::ops::Deref for PooledItem<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.item.as_ref().unwrap()
    }
}

impl<T> std::ops::DerefMut for PooledItem<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.item.as_mut().unwrap()
    }
}

impl<T> Drop for PooledItem<T> {
    fn drop(&mut self) {
        if let Some(item) = self.item.take() {
            self.pool.put(item);
        }
    }
}

/// Buffer pool specifically for DNS packet operations
pub struct BufferPool {
    pool: Pool<Vec<u8>>,
}

impl BufferPool {
    pub fn new(buffer_size: usize, max_buffers: usize) -> Self {
        let pool = Pool::new(
            move || vec![0u8; buffer_size],
            |buf| buf.clear(),
            max_buffers,
        );

        Self { pool }
    }

    pub fn get(&self) -> PooledItem<Vec<u8>> {
        self.pool.get()
    }
}

impl Clone for BufferPool {
    fn clone(&self) -> Self {
        Self {
            pool: self.pool.clone(),
        }
    }
}

/// String pool for interning common domain names
pub struct StringPool {
    pool: Pool<String>,
}

impl StringPool {
    pub fn new(max_strings: usize) -> Self {
        let pool = Pool::new(|| String::with_capacity(256), |s| s.clear(), max_strings);

        Self { pool }
    }

    pub fn get(&self) -> PooledItem<String> {
        self.pool.get()
    }
}

impl Clone for StringPool {
    fn clone(&self) -> Self {
        Self {
            pool: self.pool.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_pool() {
        let pool = BufferPool::new(1024, 10);

        // Get a buffer
        let mut buf1 = pool.get();
        buf1.extend_from_slice(b"test");
        assert_eq!(&buf1[..4], b"test");

        // Drop the buffer (returns to pool)
        drop(buf1);

        // Get another buffer - should be the same one, cleared
        let buf2 = pool.get();
        assert_eq!(buf2.len(), 0);
        assert_eq!(buf2.capacity(), 1024);
    }

    #[test]
    fn test_string_pool() {
        let pool = StringPool::new(10);

        // Get a string
        let mut s1 = pool.get();
        s1.push_str("example.com");
        assert_eq!(&*s1, "example.com");

        // Drop the string (returns to pool)
        drop(s1);

        // Get another string - should be the same one, cleared
        let s2 = pool.get();
        assert_eq!(&*s2, "");
        assert!(s2.capacity() >= 256);
    }
}

/// String interner for caching common domain names
#[derive(Debug)]
pub struct StringInterner {
    /// Map from string to interned Arc<str>
    strings: DashMap<String, Arc<str>>,
    /// Maximum number of interned strings
    max_size: usize,
}

impl StringInterner {
    pub fn new(max_size: usize) -> Self {
        Self {
            strings: DashMap::new(),
            max_size,
        }
    }

    /// Intern a string and return a reference-counted pointer
    pub fn intern(&self, s: &str) -> Arc<str> {
        // Fast path: check if already interned
        if let Some(interned) = self.strings.get(s) {
            return Arc::clone(&interned);
        }

        // Slow path: intern the string
        let interned: Arc<str> = Arc::from(s);

        // Check size limit
        if self.strings.len() < self.max_size {
            self.strings.insert(s.to_string(), Arc::clone(&interned));
        }

        interned
    }

    /// Get the number of interned strings
    pub fn len(&self) -> usize {
        self.strings.len()
    }

    /// Check if interner is empty
    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }

    /// Clear all interned strings
    pub fn clear(&self) {
        self.strings.clear();
    }
}

impl Clone for StringInterner {
    fn clone(&self) -> Self {
        Self {
            strings: DashMap::new(),
            max_size: self.max_size,
        }
    }
}
