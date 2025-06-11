# ADR-001: DNS Parser Design and Implementation

## Status
Accepted

## Context
DNS packet parsing is the core operation of any DNS server, executed millions of times per day. The parser must handle:
- Complex packet structures with variable-length fields
- DNS compression pointers that reference arbitrary packet locations
- Malformed packets from untrusted sources
- High-performance requirements (sub-millisecond responses)
- Both UDP and TCP transport protocols

Traditional approaches often involve:
1. **Full deserialization**: Parse entire packet into heap-allocated structures
2. **Streaming parsers**: Process packet incrementally with state machines
3. **Code generation**: Use tools like `nom` or protocol buffer generators

## Decision
We implemented an **in-place, zero-copy DNS parser** with the following key design choices:

### 1. In-Place Parsing with Buffer References
```rust
pub struct DNSPacket {
    buffer: Vec<u8>,
    header: DNSHeader,
    // Lazy-loaded components
    questions: Option<Vec<Question>>,
    answers: Option<Vec<ResourceRecord>>,
    // ...
}
```
- Parse directly from the input buffer without intermediate allocations
- Keep references to buffer positions rather than copying data
- Lazy-load packet sections only when accessed

### 2. Compression Pointer Handling
```rust
pub fn parse_domain_name(buffer: &[u8], offset: &mut usize, full_packet: &[u8]) -> Result<String, Box<dyn Error>>
```
- Pass full packet buffer to all parsing functions
- Recursively resolve compression pointers with loop detection
- Expand compression pointers during serialization (simpler than re-compression)

### 3. Zero-Copy Optimizations
- **Buffer Pooling**: Reuse 4KB buffers for UDP packets
- **SIMD-Style Operations**: Optimized scalar operations for pattern matching
- **Reference-Based Parsing**: Return slices instead of owned data where possible

### 4. Two-Phase Parsing Strategy
1. **Quick Scan**: Parse header and count sections
2. **Selective Parse**: Only parse needed sections based on query type

## Consequences

### Positive
- **Performance**: 6.8x faster parsing (0.09μs vs 0.63μs per packet)
- **Memory Efficiency**: ~90% reduction in allocations for typical queries
- **Cache Friendly**: Minimal memory footprint improves CPU cache utilization
- **Flexibility**: Can parse partially or fully based on needs

### Negative
- **Complexity**: Lifetime management with buffer references
- **No Compression on Write**: Responses are larger without compression
- **Buffer Lifetime**: Must keep original buffer alive during packet lifetime

### Trade-offs Accepted
1. **Code Complexity vs Performance**: More complex parsing code for significant performance gains
2. **Response Size vs Simplicity**: Larger responses without compression for simpler serialization
3. **Safety vs Speed**: Extensive bounds checking adds ~10% overhead but prevents crashes

## Benchmarks
```
DNS Packet Parsing:
- Zero-copy parse: 89.45 ns/packet
- Regular parse: 632.1 ns/packet
- Speedup: 6.8x

Cache Operations:
- Cache hit: 257 ns/lookup
- Pattern matching: 10-95 ns/operation
```

## Implementation Details

### Core Parser Structure
```rust
impl DNSPacket {
    pub fn parse(buffer: Vec<u8>) -> Result<Self, Box<dyn Error>> {
        // 1. Parse fixed header (12 bytes)
        let header = DNSHeader::parse(&buffer)?;
        
        // 2. Create packet with lazy components
        Ok(DNSPacket {
            buffer,
            header,
            questions: None,
            answers: None,
            authorities: None,
            additionals: None,
        })
    }
}
```

### Compression Pointer Safety
```rust
const MAX_COMPRESSION_JUMPS: usize = 100;

fn resolve_compression_pointer(buffer: &[u8], offset: usize, jumps: usize) -> Result<String> {
    if jumps > MAX_COMPRESSION_JUMPS {
        return Err("Compression loop detected");
    }
    // ... recursive resolution
}
```

## Alternatives Considered

### 1. Full Deserialization (Rejected)
- **Pros**: Simple API, easy to manipulate
- **Cons**: High allocation overhead, slower performance
- **Reason**: Performance requirements incompatible with allocation costs

### 2. Streaming Parser with `nom` (Rejected)
- **Pros**: Composable, well-tested
- **Cons**: Additional dependency, harder to optimize
- **Reason**: Need fine-grained control over parsing strategy

### 3. Code Generation (Rejected)
- **Pros**: Type-safe, potentially optimal code
- **Cons**: Build complexity, less flexible
- **Reason**: DNS packets too dynamic for static generation

## References
- RFC 1035: Domain Names - Implementation and Specification
- RFC 3597: Handling of Unknown DNS Resource Record Types
- Internal benchmarks: `/benches/dns_parsing.rs`
- Performance analysis: `/docs/PERFORMANCE_TUNING.md`