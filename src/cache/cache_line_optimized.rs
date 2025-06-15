use super::{CacheEntry, CacheKey};
use crate::dns::DNSPacket;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Cache entry optimized for CPU cache line efficiency
/// Hot data (frequently accessed) is separated from cold data
#[repr(C)]
pub struct CacheLineOptimizedEntry {
    /// Hot section - 64 bytes aligned
    hot: HotData,
    /// Cold section - rarely accessed
    cold: ColdData,
}

/// Frequently accessed data - fits in one cache line (64 bytes)
#[repr(C, align(64))]
struct HotData {
    /// Entry validity and access tracking
    last_access_time: AtomicU64,    // 8 bytes
    hit_count: AtomicU32,           // 4 bytes
    ttl_seconds: AtomicU32,         // 4 bytes
    /// Expiry timestamp (seconds since epoch)
    expiry_timestamp: AtomicU64,    // 8 bytes
    /// Flags (is_negative, etc.)
    flags: AtomicU32,               // 4 bytes
    /// Hash of the response for quick validation
    response_hash: AtomicU64,       // 8 bytes
    /// Size of serialized response
    response_size: AtomicU32,       // 4 bytes
    /// Reserved for future use
    _reserved: AtomicU32,           // 4 bytes
    /// Padding to ensure 64-byte alignment
    _padding: [u8; 16],             // 16 bytes
}

/// Infrequently accessed data
#[repr(C)]
struct ColdData {
    /// The actual DNS response packet
    response: parking_lot::RwLock<DNSPacket>,
    /// Original TTL for cache persistence
    original_ttl: u32,
    /// Creation timestamp
    created_at: Instant,
}

// Flags for the hot data
const FLAG_IS_NEGATIVE: u32 = 1 << 0;
const FLAG_IS_EXPIRED: u32 = 1 << 1;

impl CacheLineOptimizedEntry {
    pub fn new(response: DNSPacket, ttl: u32, is_negative: bool) -> Self {
        let now = Instant::now();
        let expiry = now + Duration::from_secs(ttl as u64);
        let expiry_timestamp = expiry.duration_since(now).as_secs();
        
        let mut flags = 0;
        if is_negative {
            flags |= FLAG_IS_NEGATIVE;
        }
        
        // Calculate a simple hash of the response for validation
        let response_hash = Self::hash_response(&response);
        let response_size = response.serialize().map(|v| v.len() as u32).unwrap_or(0);
        
        Self {
            hot: HotData {
                last_access_time: AtomicU64::new(0),
                hit_count: AtomicU32::new(0),
                ttl_seconds: AtomicU32::new(ttl),
                expiry_timestamp: AtomicU64::new(expiry_timestamp),
                flags: AtomicU32::new(flags),
                response_hash: AtomicU64::new(response_hash),
                response_size: AtomicU32::new(response_size),
                _reserved: AtomicU32::new(0),
                _padding: [0; 16],
            },
            cold: ColdData {
                response: parking_lot::RwLock::new(response),
                original_ttl: ttl,
                created_at: now,
            },
        }
    }
    
    /// Check if entry is valid without accessing cold data
    #[inline]
    pub fn is_valid(&self) -> bool {
        let flags = self.hot.flags.load(Ordering::Relaxed);
        if flags & FLAG_IS_EXPIRED != 0 {
            return false;
        }
        
        let expiry = self.hot.expiry_timestamp.load(Ordering::Relaxed);
        let now_secs = Instant::now().elapsed().as_secs();
        
        if now_secs >= expiry {
            // Mark as expired for future checks
            self.hot.flags.fetch_or(FLAG_IS_EXPIRED, Ordering::Relaxed);
            false
        } else {
            true
        }
    }
    
    /// Record an access to this entry
    #[inline]
    pub fn record_access(&self) {
        self.hot.hit_count.fetch_add(1, Ordering::Relaxed);
        self.hot.last_access_time.store(
            Instant::now().elapsed().as_micros() as u64,
            Ordering::Relaxed
        );
    }
    
    /// Get the response if valid
    pub fn get_response(&self) -> Option<DNSPacket> {
        if !self.is_valid() {
            return None;
        }
        
        self.record_access();
        
        // Only access cold data if entry is valid
        let response = self.cold.response.read().clone();
        
        // Validate response hasn't been corrupted
        let current_hash = Self::hash_response(&response);
        let stored_hash = self.hot.response_hash.load(Ordering::Relaxed);
        
        if current_hash == stored_hash {
            Some(response)
        } else {
            None
        }
    }
    
    /// Get hit count for LFU eviction
    #[inline]
    pub fn hit_count(&self) -> u32 {
        self.hot.hit_count.load(Ordering::Relaxed)
    }
    
    /// Get last access time for LRU eviction
    #[inline]
    pub fn last_access_time(&self) -> u64 {
        self.hot.last_access_time.load(Ordering::Relaxed)
    }
    
    /// Check if this is a negative cache entry
    #[inline]
    pub fn is_negative(&self) -> bool {
        self.hot.flags.load(Ordering::Relaxed) & FLAG_IS_NEGATIVE != 0
    }
    
    /// Simple hash function for response validation
    fn hash_response(response: &DNSPacket) -> u64 {
        use std::hash::{Hash, Hasher};
        use std::collections::hash_map::DefaultHasher;
        
        let mut hasher = DefaultHasher::new();
        response.header.id.hash(&mut hasher);
        response.header.qr.hash(&mut hasher);
        response.header.opcode.hash(&mut hasher);
        response.header.rcode.hash(&mut hasher);
        response.questions.len().hash(&mut hasher);
        response.answers.len().hash(&mut hasher);
        
        hasher.finish()
    }
}

/// Wrapper to make it compatible with existing cache interface
impl From<CacheLineOptimizedEntry> for CacheEntry {
    fn from(opt: CacheLineOptimizedEntry) -> Self {
        let response = opt.cold.response.read().clone();
        CacheEntry::new(
            response,
            opt.cold.original_ttl,
            opt.is_negative(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_cache_line_alignment() {
        // Verify HotData is exactly 64 bytes
        assert_eq!(
            std::mem::size_of::<HotData>(),
            64,
            "HotData should be exactly 64 bytes"
        );
        
        // Verify alignment
        assert_eq!(
            std::mem::align_of::<HotData>(),
            64,
            "HotData should be aligned to 64 bytes"
        );
    }
    
    #[test]
    fn test_entry_operations() {
        let mut packet = DNSPacket::default();
        packet.header.id = 12345;
        
        let entry = CacheLineOptimizedEntry::new(packet, 300, false);
        
        // Test validity
        assert!(entry.is_valid());
        assert!(!entry.is_negative());
        
        // Test access recording
        let initial_hits = entry.hit_count();
        entry.record_access();
        assert_eq!(entry.hit_count(), initial_hits + 1);
        
        // Test response retrieval
        let response = entry.get_response().unwrap();
        assert_eq!(response.header.id, 12345);
    }
}