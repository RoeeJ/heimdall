use std::cell::RefCell;
use std::mem;
use std::rc::Rc;

/// Buffer size for DNS packets (4KB is typical max UDP size with EDNS)
const DNS_BUFFER_SIZE: usize = 4096;

/// Maximum number of buffers to keep in thread-local pool
const MAX_BUFFERS_PER_THREAD: usize = 16;

thread_local! {
    /// Thread-local buffer pool
    static BUFFER_POOL: Rc<RefCell<BufferPool>> = Rc::new(RefCell::new(BufferPool::new()));
}

/// A pooled buffer that returns itself to the pool when dropped
pub struct PooledBuffer {
    buffer: Option<Vec<u8>>,
    pool: Rc<RefCell<BufferPool>>,
}

impl PooledBuffer {
    /// Get the buffer capacity
    pub fn capacity(&self) -> usize {
        self.buffer.as_ref().map(|b| b.capacity()).unwrap_or(0)
    }

    /// Clear the buffer
    pub fn clear(&mut self) {
        if let Some(buf) = &mut self.buffer {
            buf.clear();
        }
    }

    /// Resize the buffer
    pub fn resize(&mut self, new_len: usize, value: u8) {
        if let Some(buf) = &mut self.buffer {
            buf.resize(new_len, value);
        }
    }
}

impl Drop for PooledBuffer {
    fn drop(&mut self) {
        if let Some(mut buffer) = self.buffer.take() {
            // Clear the buffer before returning to pool
            buffer.clear();

            // Only return to pool if it's the standard size and pool isn't full
            if buffer.capacity() == DNS_BUFFER_SIZE {
                if let Ok(mut pool) = self.pool.try_borrow_mut() {
                    pool.return_buffer(buffer);
                }
            }
        }
    }
}

impl std::ops::Deref for PooledBuffer {
    type Target = Vec<u8>;

    fn deref(&self) -> &Self::Target {
        self.buffer.as_ref().expect("Buffer should not be None")
    }
}

impl std::ops::DerefMut for PooledBuffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.buffer.as_mut().expect("Buffer should not be None")
    }
}

/// Thread-local buffer pool
struct BufferPool {
    buffers: Vec<Vec<u8>>,
    allocated_count: usize,
    reuse_count: usize,
}

impl BufferPool {
    fn new() -> Self {
        Self {
            buffers: Vec::with_capacity(MAX_BUFFERS_PER_THREAD),
            allocated_count: 0,
            reuse_count: 0,
        }
    }

    fn get_buffer(&mut self) -> Vec<u8> {
        if let Some(buffer) = self.buffers.pop() {
            self.reuse_count += 1;
            buffer
        } else {
            self.allocated_count += 1;
            Vec::with_capacity(DNS_BUFFER_SIZE)
        }
    }

    fn return_buffer(&mut self, buffer: Vec<u8>) {
        if self.buffers.len() < MAX_BUFFERS_PER_THREAD {
            self.buffers.push(buffer);
        }
        // Otherwise, let the buffer be dropped
    }

    /// Get statistics about the pool
    #[allow(dead_code)]
    fn stats(&self) -> (usize, usize, usize) {
        (self.buffers.len(), self.allocated_count, self.reuse_count)
    }
}

/// Get a buffer from the thread-local pool
pub fn get_pooled_buffer() -> PooledBuffer {
    BUFFER_POOL.with(|pool_cell| {
        // Need to clone the Rc, not create a new one from a reference
        let buffer = pool_cell.borrow_mut().get_buffer();

        PooledBuffer {
            buffer: Some(buffer),
            pool: pool_cell.clone(),
        }
    })
}

/// Get a pre-sized buffer from the thread-local pool
pub fn get_pooled_buffer_sized(size: usize) -> PooledBuffer {
    let mut buffer = get_pooled_buffer();
    buffer.resize(size, 0);
    buffer
}

/// Optimized buffer swap for zero-copy operations
pub fn swap_pooled_buffer(buffer: &mut PooledBuffer) -> Vec<u8> {
    let new_buffer = {
        let mut pool = buffer.pool.borrow_mut();
        pool.get_buffer()
    };

    // Swap the buffers
    let old_buffer = mem::replace(&mut buffer.buffer, Some(new_buffer));
    old_buffer.expect("Buffer should not be None")
}

/// Get thread-local pool statistics
#[allow(dead_code)]
pub fn get_thread_pool_stats() -> (usize, usize, usize) {
    BUFFER_POOL.with(|pool_cell| pool_cell.borrow().stats())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_pool_basic() {
        let mut buffer = get_pooled_buffer();
        assert_eq!(buffer.capacity(), DNS_BUFFER_SIZE);
        assert_eq!(buffer.len(), 0);

        // Use the buffer
        buffer.extend_from_slice(b"test data");
        assert_eq!(&buffer[..], b"test data");
    }

    #[test]
    fn test_buffer_reuse() {
        // Get initial stats
        let (pool_size_1, allocated_1, _) = get_thread_pool_stats();

        {
            let _buffer1 = get_pooled_buffer();
            let _buffer2 = get_pooled_buffer();
            // Buffers will be returned when dropped
        }

        // Check that buffers were returned to pool
        let (pool_size_2, allocated_2, _) = get_thread_pool_stats();
        assert!(pool_size_2 >= pool_size_1);
        assert!(allocated_2 >= allocated_1);

        // Get buffer again - should reuse
        let _buffer3 = get_pooled_buffer();
        let (_, _, reuse_count) = get_thread_pool_stats();
        assert!(reuse_count > 0);
    }

    #[test]
    fn test_buffer_clear_on_return() {
        {
            let mut buffer = get_pooled_buffer();
            buffer.extend_from_slice(b"sensitive data");
            assert!(!buffer.is_empty());
            // Buffer dropped here
        }

        // Get another buffer - should be cleared
        let buffer = get_pooled_buffer();
        assert!(buffer.is_empty());
    }

    #[test]
    fn test_sized_buffer() {
        let buffer = get_pooled_buffer_sized(1024);
        assert_eq!(buffer.len(), 1024);
        assert_eq!(buffer.capacity(), DNS_BUFFER_SIZE);
        assert!(buffer.iter().all(|&b| b == 0));
    }

    #[test]
    fn test_buffer_swap() {
        let mut buffer1 = get_pooled_buffer();
        buffer1.extend_from_slice(b"original data");

        let old_data = swap_pooled_buffer(&mut buffer1);
        assert_eq!(&old_data[..], b"original data");
        assert!(buffer1.is_empty());
    }

    #[test]
    fn test_pool_limit() {
        // Allocate more buffers than the pool limit
        let mut buffers = Vec::new();
        for _ in 0..MAX_BUFFERS_PER_THREAD + 5 {
            buffers.push(get_pooled_buffer());
        }

        // Drop all buffers
        buffers.clear();

        // Pool should only keep up to MAX_BUFFERS_PER_THREAD
        let (pool_size, _, _) = get_thread_pool_stats();
        assert!(pool_size <= MAX_BUFFERS_PER_THREAD);
    }
}
