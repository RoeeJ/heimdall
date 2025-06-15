use dashmap::DashMap;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::hash::Hash;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Segmented LRU (SLRU) cache implementation
/// Divides cache into two segments: probationary and protected
/// This reduces cache pollution from one-time accesses
pub struct SlruCache<K, V> {
    /// Protected segment - frequently accessed items
    protected: Arc<DashMap<K, V>>,
    /// Probationary segment - recently accessed items
    probationary: Arc<DashMap<K, V>>,
    /// Order tracking for protected segment
    protected_order: Arc<Mutex<VecDeque<K>>>,
    /// Order tracking for probationary segment
    probationary_order: Arc<Mutex<VecDeque<K>>>,
    /// Maximum size of protected segment
    protected_size: usize,
    /// Maximum size of probationary segment
    probationary_size: usize,
    /// Current sizes (atomic for lock-free reads)
    protected_count: AtomicUsize,
    probationary_count: AtomicUsize,
}

impl<K, V> SlruCache<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    pub fn new(total_capacity: usize) -> Self {
        // Split capacity: 20% probationary, 80% protected
        let probationary_size = total_capacity / 5;
        let protected_size = total_capacity - probationary_size;
        
        Self {
            protected: Arc::new(DashMap::with_capacity(protected_size)),
            probationary: Arc::new(DashMap::with_capacity(probationary_size)),
            protected_order: Arc::new(Mutex::new(VecDeque::with_capacity(protected_size))),
            probationary_order: Arc::new(Mutex::new(VecDeque::with_capacity(probationary_size))),
            protected_size,
            probationary_size,
            protected_count: AtomicUsize::new(0),
            probationary_count: AtomicUsize::new(0),
        }
    }
    
    /// Get a value from the cache
    pub fn get(&self, key: &K) -> Option<V> {
        // Check protected segment first
        if let Some(entry) = self.protected.get(key) {
            // Move to front of protected LRU
            self.update_protected_lru(key);
            return Some(entry.clone());
        }
        
        // Check probationary segment
        if let Some(entry) = self.probationary.get(key) {
            let value = entry.clone();
            drop(entry); // Release the reference
            
            // Promote to protected segment
            self.promote_to_protected(key.clone(), value.clone());
            return Some(value);
        }
        
        None
    }
    
    /// Insert a value into the cache
    pub fn insert(&self, key: K, value: V) {
        // Check if already in protected
        if self.protected.contains_key(&key) {
            self.protected.insert(key.clone(), value);
            self.update_protected_lru(&key);
            return;
        }
        
        // Check if already in probationary
        if self.probationary.contains_key(&key) {
            // Promote to protected
            self.probationary.remove(&key);
            self.probationary_count.fetch_sub(1, Ordering::Relaxed);
            self.remove_from_probationary_order(&key);
            self.promote_to_protected(key, value);
            return;
        }
        
        // New entry - add to probationary
        self.insert_probationary(key, value);
    }
    
    /// Insert into probationary segment
    fn insert_probationary(&self, key: K, value: V) {
        // Check if probationary is full
        if self.probationary_count.load(Ordering::Relaxed) >= self.probationary_size {
            // Evict from probationary
            if let Some(evict_key) = self.probationary_order.lock().pop_front() {
                self.probationary.remove(&evict_key);
                self.probationary_count.fetch_sub(1, Ordering::Relaxed);
            }
        }
        
        self.probationary.insert(key.clone(), value);
        self.probationary_order.lock().push_back(key);
        self.probationary_count.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Promote entry from probationary to protected
    fn promote_to_protected(&self, key: K, value: V) {
        // Remove from probationary
        self.probationary.remove(&key);
        self.probationary_count.fetch_sub(1, Ordering::Relaxed);
        self.remove_from_probationary_order(&key);
        
        // Check if protected is full
        if self.protected_count.load(Ordering::Relaxed) >= self.protected_size {
            // Evict from protected to probationary
            if let Some(evict_key) = self.protected_order.lock().pop_front() {
                if let Some((k, v)) = self.protected.remove(&evict_key) {
                    self.protected_count.fetch_sub(1, Ordering::Relaxed);
                    // Demote to probationary
                    self.insert_probationary(k, v);
                }
            }
        }
        
        // Insert into protected
        self.protected.insert(key.clone(), value);
        self.protected_order.lock().push_back(key);
        self.protected_count.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Update LRU order for protected segment
    fn update_protected_lru(&self, key: &K) {
        let mut order = self.protected_order.lock();
        // Remove from current position
        order.retain(|k| k != key);
        // Add to back (most recently used)
        order.push_back(key.clone());
    }
    
    /// Remove from probationary order tracking
    fn remove_from_probationary_order(&self, key: &K) {
        let mut order = self.probationary_order.lock();
        order.retain(|k| k != key);
    }
    
    /// Get current cache size
    pub fn len(&self) -> usize {
        self.protected_count.load(Ordering::Relaxed) + 
        self.probationary_count.load(Ordering::Relaxed)
    }
    
    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    
    /// Clear the cache
    pub fn clear(&self) {
        self.protected.clear();
        self.probationary.clear();
        self.protected_order.lock().clear();
        self.probationary_order.lock().clear();
        self.protected_count.store(0, Ordering::Relaxed);
        self.probationary_count.store(0, Ordering::Relaxed);
    }
    
    /// Get statistics about the cache
    pub fn stats(&self) -> SlruStats {
        SlruStats {
            protected_size: self.protected_count.load(Ordering::Relaxed),
            probationary_size: self.probationary_count.load(Ordering::Relaxed),
            protected_capacity: self.protected_size,
            probationary_capacity: self.probationary_size,
        }
    }
}

#[derive(Debug)]
pub struct SlruStats {
    pub protected_size: usize,
    pub probationary_size: usize,
    pub protected_capacity: usize,
    pub probationary_capacity: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_slru_basic() {
        let cache = SlruCache::new(100);
        
        // Insert items
        cache.insert("a", 1);
        cache.insert("b", 2);
        cache.insert("c", 3);
        
        // All should be in probationary
        let stats = cache.stats();
        assert_eq!(stats.probationary_size, 3);
        assert_eq!(stats.protected_size, 0);
        
        // Access "a" again - should promote to protected
        assert_eq!(cache.get(&"a"), Some(1));
        
        let stats = cache.stats();
        assert_eq!(stats.probationary_size, 2);
        assert_eq!(stats.protected_size, 1);
    }
    
    #[test]
    fn test_slru_eviction() {
        let cache = SlruCache::new(5); // Small cache for testing
        
        // Fill probationary (capacity = 1)
        cache.insert("a", 1);
        cache.insert("b", 2); // Should evict "a"
        
        assert_eq!(cache.get(&"a"), None);
        assert_eq!(cache.get(&"b"), Some(2));
        
        // Promote "b" to protected
        assert_eq!(cache.get(&"b"), Some(2));
        
        // Fill protected
        cache.insert("c", 3);
        cache.get(&"c"); // Promote
        cache.insert("d", 4);
        cache.get(&"d"); // Promote
        cache.insert("e", 5);
        cache.get(&"e"); // Promote
        cache.insert("f", 6);
        cache.get(&"f"); // Promote - should demote "b"
        
        let stats = cache.stats();
        assert_eq!(stats.protected_size, 4); // Protected is full
    }
}