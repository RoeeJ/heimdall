# Dead Code Analysis for Heimdall DNS Server

## Summary of Findings

After a comprehensive analysis of the Heimdall codebase, I found several instances of dead code that could potentially be removed:

## 1. Functions Marked with `#[allow(dead_code)]` That Are Never Used

### `/src/blocking/trie.rs`
- **Line 330**: `hash_label_with_salt()` - This function is defined but never called anywhere in the codebase
- **Line 349**: `find_or_add_to_arena()` - This function is defined but never called anywhere in the codebase

### `/src/blocking/psl.rs`
- **Line 190**: `find_public_suffix_len()` - This function is defined but never called anywhere in the codebase

## 2. Struct Fields Marked with `#[allow(dead_code)]`

### `/src/blocking/blocker_v2.rs`
- **Line 24**: `arena: Arc<SharedArena>` - Field is stored but appears unused

### `/src/cache/redis_backend.rs`
- **Line 63**: `default_ttl: u64` - Field is stored but might not be used (needs deeper analysis)

### `/src/cache/optimized.rs`
- **Line 15**: `max_size: usize` - Field might be unused
- **Line 23**: `cache_file_path: Option<String>` - Field might be unused

## 3. Potentially Unused Constants

### `/src/zone/mod.rs`
- **Lines 42-45**: Several `DEFAULT_SOA_*` constants that don't appear to be used:
  - `DEFAULT_SOA_REFRESH`
  - `DEFAULT_SOA_RETRY`
  - `DEFAULT_SOA_EXPIRE`
  - `DEFAULT_SOA_MINIMUM`

## 4. Fields That Might Be Falsely Marked as Dead Code

### `/src/resolver.rs`
- **Line 271**: `client_socket` - Actually used in the code
- **Line 283**: `query_counter` - Actually used with `fetch_add` and `load` operations
- **Line 281**: `metrics` - Needs verification but likely used

## Recommendations

1. **Remove genuinely dead functions**:
   - `hash_label_with_salt()` in trie.rs
   - `find_or_add_to_arena()` in trie.rs
   - `find_public_suffix_len()` in psl.rs

2. **Review and potentially remove unused constants** in zone/mod.rs

3. **Remove false positive `#[allow(dead_code)]` annotations** for fields that are actually used:
   - `client_socket` in resolver.rs
   - `query_counter` in resolver.rs

4. **Investigate fields that might be used indirectly**:
   - Redis backend fields might be used through trait implementations
   - Cache optimization fields might be used for future features

## Notes

- No significant blocks of commented-out code were found
- No unused imports were detected by the compiler
- Most type aliases and structs are actively used
- The codebase is generally well-maintained with minimal dead code