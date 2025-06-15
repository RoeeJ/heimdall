# Optimization Implementation Guide

## Phase 1 Implementation Details

### 1.1 Zero-Copy Parsing Implementation

#### Current Code Analysis
```rust
// Current: Multiple allocations
pub fn parse(data: &[u8]) -> Result<DNSPacket, DNSError> {
    let mut packet = DNSPacket::default();
    // Allocates Vec for labels, questions, etc.
}
```

#### Zero-Copy Approach
```rust
// Proposed: Lifetime-based parsing
pub struct DNSPacket<'a> {
    raw_data: &'a [u8],
    header: DNSHeader,
    questions: LazyVec<'a, DNSQuestion<'a>>,
    // Parse on demand, store offsets
}

impl<'a> DNSPacket<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self, DNSError> {
        // Parse header directly from bytes
        let header = DNSHeader::from_bytes(&data[0..12])?;
        
        Ok(DNSPacket {
            raw_data: data,
            header,
            questions: LazyVec::new(data, 12),
        })
    }
}
```

#### Implementation Steps
1. Create `LazyVec` type for on-demand parsing
2. Store byte offsets instead of parsed data
3. Implement zero-copy label parsing
4. Use `Cow<'a, str>` for domain names

### 1.2 Thread-Local Buffer Pool Implementation

#### Buffer Pool Design
```rust
use std::cell::RefCell;
use std::collections::VecDeque;

thread_local! {
    static BUFFER_POOL_512: RefCell<BufferPool> = RefCell::new(BufferPool::new(512, 100));
    static BUFFER_POOL_1K: RefCell<BufferPool> = RefCell::new(BufferPool::new(1024, 50));
    static BUFFER_POOL_4K: RefCell<BufferPool> = RefCell::new(BufferPool::new(4096, 20));
}

struct BufferPool {
    size: usize,
    max_buffers: usize,
    buffers: VecDeque<Vec<u8>>,
}

impl BufferPool {
    fn acquire(&mut self) -> Vec<u8> {
        self.buffers.pop_front()
            .unwrap_or_else(|| vec![0u8; self.size])
    }
    
    fn release(&mut self, mut buffer: Vec<u8>) {
        if self.buffers.len() < self.max_buffers {
            buffer.clear();
            buffer.resize(self.size, 0);
            self.buffers.push_back(buffer);
        }
    }
}

// Usage in UDP handler
pub async fn handle_udp_query(socket: &UdpSocket) -> Result<(), Error> {
    let mut buffer = BUFFER_POOL_4K.with(|pool| pool.borrow_mut().acquire());
    
    let (len, addr) = socket.recv_from(&mut buffer).await?;
    // Process query...
    
    BUFFER_POOL_4K.with(|pool| pool.borrow_mut().release(buffer));
}
```

### 1.3 Hot Path Optimizations

#### Profile-Guided Optimizations
```rust
// 1. Inline critical functions
#[inline(always)]
fn parse_domain_name(data: &[u8], offset: usize) -> Result<(String, usize), DNSError> {
    // Critical path - always inline
}

// 2. Remove bounds checks in hot loops
unsafe fn parse_labels_unchecked(data: &[u8], offset: usize) -> Vec<String> {
    // Use unsafe for performance after validation
}

// 3. Optimize domain comparisons
fn domain_equals_fast(a: &str, b: &str) -> bool {
    // Case-insensitive comparison with SIMD
    a.len() == b.len() && a.as_bytes().eq_ignore_ascii_case(b.as_bytes())
}
```

## Phase 2 Implementation Details

### 2.1 Lock-Free Cache with evmap

```rust
use evmap::{ReadHandle, WriteHandle};
use std::sync::Arc;

pub struct LockFreeCache {
    read: ReadHandle<String, CacheEntry>,
    write: Arc<Mutex<WriteHandle<String, CacheEntry>>>,
}

impl LockFreeCache {
    pub fn new() -> Self {
        let (read, write) = evmap::new();
        Self {
            read,
            write: Arc::new(Mutex::new(write)),
        }
    }
    
    pub fn get(&self, key: &str) -> Option<CacheEntry> {
        self.read.get_one(key).cloned()
    }
    
    pub fn insert(&self, key: String, value: CacheEntry) {
        let mut write = self.write.lock().unwrap();
        write.insert(key, value);
        write.publish();
    }
}
```

### 2.2 Cache Line Optimization

```rust
#[repr(C, align(64))] // Cache line aligned
struct OptimizedCacheEntry {
    // Hot data (frequently accessed)
    ttl: AtomicU32,
    hit_count: AtomicU32,
    last_access: AtomicU64,
    
    // Padding to cache line boundary
    _pad: [u8; 16],
    
    // Cold data (rarely accessed)
    packet_data: Vec<u8>,
    created_at: Instant,
}
```

## Phase 3 Implementation Details

### 3.1 Multiple UDP Sockets with SO_REUSEPORT

```rust
use nix::sys::socket::{setsockopt, sockopt::ReusePort};

fn create_reuse_port_socket() -> Result<UdpSocket> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0")?;
    let raw_fd = socket.as_raw_fd();
    
    // Enable SO_REUSEPORT
    setsockopt(raw_fd, ReusePort, &true)?;
    
    socket.bind("127.0.0.1:1053")?;
    Ok(UdpSocket::from_std(socket)?)
}

// Create one socket per CPU core
let num_cores = num_cpus::get();
let sockets: Vec<UdpSocket> = (0..num_cores)
    .map(|_| create_reuse_port_socket())
    .collect::<Result<Vec<_>>>()?;
```

### 3.2 Batch Processing with recvmmsg

```rust
use libc::{recvmmsg, mmsghdr, iovec};

const BATCH_SIZE: usize = 32;

unsafe fn batch_receive(socket: &UdpSocket) -> Vec<(Vec<u8>, SocketAddr)> {
    let mut msgs: Vec<mmsghdr> = vec![std::mem::zeroed(); BATCH_SIZE];
    let mut iovecs: Vec<iovec> = vec![std::mem::zeroed(); BATCH_SIZE];
    let mut buffers: Vec<Vec<u8>> = (0..BATCH_SIZE)
        .map(|_| vec![0u8; 4096])
        .collect();
    
    // Setup iovecs and msgs
    for i in 0..BATCH_SIZE {
        iovecs[i].iov_base = buffers[i].as_mut_ptr() as *mut _;
        iovecs[i].iov_len = buffers[i].len();
        msgs[i].msg_hdr.msg_iov = &mut iovecs[i];
        msgs[i].msg_hdr.msg_iovlen = 1;
    }
    
    let n = recvmmsg(
        socket.as_raw_fd(),
        msgs.as_mut_ptr(),
        BATCH_SIZE as u32,
        0,
        std::ptr::null_mut(),
    );
    
    // Process received messages
    // ...
}
```

## Phase 4 SIMD Implementation

### Domain Name Comparison with AVX2

```rust
use std::arch::x86_64::*;

#[target_feature(enable = "avx2")]
unsafe fn compare_domain_simd(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    
    let chunks = a.len() / 32;
    let remainder = a.len() % 32;
    
    // Process 32 bytes at a time
    for i in 0..chunks {
        let a_vec = _mm256_loadu_si256(a[i*32..].as_ptr() as *const _);
        let b_vec = _mm256_loadu_si256(b[i*32..].as_ptr() as *const _);
        
        // Case-insensitive comparison
        let a_lower = to_lowercase_avx2(a_vec);
        let b_lower = to_lowercase_avx2(b_vec);
        
        let cmp = _mm256_cmpeq_epi8(a_lower, b_lower);
        let mask = _mm256_movemask_epi8(cmp);
        
        if mask != -1 {
            return false;
        }
    }
    
    // Handle remainder
    a[chunks*32..].eq_ignore_ascii_case(&b[chunks*32..])
}
```

## Performance Testing Framework

### Automated Performance Regression Tests

```rust
#[cfg(test)]
mod perf_tests {
    use criterion::{black_box, criterion_group, criterion_main, Criterion};
    
    fn benchmark_packet_parsing(c: &mut Criterion) {
        let packet_data = create_test_packet();
        
        c.bench_function("parse_packet", |b| {
            b.iter(|| {
                let packet = DNSPacket::parse(black_box(&packet_data));
                black_box(packet);
            })
        });
    }
    
    fn benchmark_cache_lookup(c: &mut Criterion) {
        let cache = create_test_cache();
        
        c.bench_function("cache_lookup", |b| {
            b.iter(|| {
                let result = cache.get(black_box("example.com"));
                black_box(result);
            })
        });
    }
    
    criterion_group!(benches, benchmark_packet_parsing, benchmark_cache_lookup);
    criterion_main!(benches);
}
```

### Continuous Performance Monitoring

```yaml
# .github/workflows/performance.yml
name: Performance Regression Tests

on: [push, pull_request]

jobs:
  benchmark:
    runs-on: ubuntu-latest
    steps:
    - uses: actions/checkout@v2
    
    - name: Run benchmarks
      run: |
        cargo bench --all-features -- --save-baseline new
        
    - name: Compare with baseline
      run: |
        cargo bench --all-features -- --baseline main --compare
        
    - name: Upload results
      uses: benchmark-action/github-action-benchmark@v1
      with:
        tool: 'cargo'
        output-file-path: target/criterion
```

## Rollout Strategy

### 1. Feature Flags
```rust
pub struct OptimizationFlags {
    pub zero_copy_parsing: bool,
    pub thread_local_buffers: bool,
    pub lock_free_cache: bool,
    pub simd_enabled: bool,
    pub io_uring_enabled: bool,
}

impl Default for OptimizationFlags {
    fn default() -> Self {
        Self {
            zero_copy_parsing: false,
            thread_local_buffers: true,  // Low risk
            lock_free_cache: false,
            simd_enabled: is_x86_feature_detected!("avx2"),
            io_uring_enabled: false,
        }
    }
}
```

### 2. Gradual Rollout
1. **Dev Testing**: Enable all optimizations in dev
2. **Staging**: Enable low-risk optimizations
3. **Production**: Gradual rollout with monitoring
4. **Full Deploy**: All optimizations enabled

### 3. Monitoring Checklist
- [ ] QPS throughput maintained or improved
- [ ] P99 latency within acceptable range
- [ ] No increase in error rates
- [ ] Memory usage stable
- [ ] CPU usage per query reduced
- [ ] No crashes or panics