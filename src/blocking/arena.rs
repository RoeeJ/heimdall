use std::sync::Arc;

/// A simple arena allocator for storing strings contiguously in memory
/// This allows us to store domains without individual allocations
pub struct StringArena {
    /// The backing buffer containing all strings
    buffer: Vec<u8>,
    /// Current write position in the buffer
    pos: usize,
}

impl StringArena {
    /// Create a new arena with the specified capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buffer: Vec::with_capacity(capacity),
            pos: 0,
        }
    }

    /// Add a string to the arena and return its offset and length
    /// Returns None if the string would exceed capacity
    pub fn add(&mut self, s: &[u8]) -> Option<(u32, u16)> {
        let len = s.len();
        if len > u16::MAX as usize {
            return None; // String too long
        }

        let start = self.pos;
        let end = start + len;

        // Ensure we have capacity
        if end > self.buffer.capacity() {
            // Try to grow the buffer
            let new_capacity = (self.buffer.capacity() * 2).max(end);
            self.buffer.reserve(new_capacity - self.buffer.capacity());
        }

        // Copy the string data
        self.buffer.extend_from_slice(s);
        self.pos = end;

        Some((start as u32, len as u16))
    }

    /// Get a string slice from the arena
    #[inline]
    pub fn get(&self, offset: u32, len: u16) -> Option<&[u8]> {
        let start = offset as usize;
        let end = start + len as usize;

        if end <= self.buffer.len() {
            Some(&self.buffer[start..end])
        } else {
            None
        }
    }

    /// Get the total size of allocated data
    pub fn size(&self) -> usize {
        self.pos
    }

    /// Convert to a shared immutable arena
    pub fn into_shared(mut self) -> SharedArena {
        self.buffer.truncate(self.pos);
        self.buffer.shrink_to_fit();
        SharedArena {
            buffer: Arc::new(self.buffer),
        }
    }
}

/// A shared, immutable version of the arena for concurrent access
#[derive(Clone)]
pub struct SharedArena {
    buffer: Arc<Vec<u8>>,
}

impl SharedArena {
    /// Get a string slice from the shared arena
    #[inline]
    pub fn get(&self, offset: u32, len: u16) -> Option<&[u8]> {
        let start = offset as usize;
        let end = start + len as usize;

        if end <= self.buffer.len() {
            Some(&self.buffer[start..end])
        } else {
            None
        }
    }

    /// Create from existing buffer (for PSL data)
    pub fn from_buffer(buffer: Vec<u8>) -> Self {
        Self {
            buffer: Arc::new(buffer),
        }
    }

    /// Get the underlying buffer
    pub fn as_bytes(&self) -> &[u8] {
        &self.buffer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_arena() {
        let mut arena = StringArena::with_capacity(1024);

        // Add some strings
        let (offset1, len1) = arena.add(b"example.com").unwrap();
        let (offset2, len2) = arena.add(b"test.example.com").unwrap();

        // Verify we can retrieve them
        assert_eq!(arena.get(offset1, len1).unwrap(), b"example.com");
        assert_eq!(arena.get(offset2, len2).unwrap(), b"test.example.com");

        // Verify offsets are sequential
        assert_eq!(offset1, 0);
        assert_eq!(offset2, 11); // "example.com".len()
    }

    #[test]
    fn test_shared_arena() {
        let mut arena = StringArena::with_capacity(1024);
        let (offset, len) = arena.add(b"example.com").unwrap();

        let shared = arena.into_shared();
        assert_eq!(shared.get(offset, len).unwrap(), b"example.com");

        // Test cloning
        let shared2 = shared.clone();
        assert_eq!(shared2.get(offset, len).unwrap(), b"example.com");
    }
}
