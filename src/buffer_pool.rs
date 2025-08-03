//! High-performance, thread-safe buffer pool for parallel I/O operations.
//!
//! This module provides a `BufferPool` that pre-allocates buffers to avoid expensive
//! runtime memory allocation/deallocation when processing large genomics files
//! across multiple threads.

use crossbeam::queue::ArrayQueue;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use thiserror::Error;

use crate::constants::{MAX_BUFFER_SIZE, MAX_POOL_CAPACITY, MAX_TOTAL_MEMORY, PAGE_SIZE};

/// Errors that can occur during buffer pool operations
#[derive(Error, Debug)]
pub enum BufferPoolError {
    #[error("Pool capacity must be positive and <= {MAX_POOL_CAPACITY}")]
    InvalidCapacity,
    #[error("Buffer size must be positive, <= {MAX_BUFFER_SIZE}, and aligned to {PAGE_SIZE} bytes")]
    InvalidBufferSize,
    #[error("Total memory usage would exceed {MAX_TOTAL_MEMORY} bytes")]
    MemoryLimitExceeded,
    #[error("Buffer has already been released (double-release detected)")]
    DoubleRelease,
}

/// Internal shared state for the buffer pool
#[derive(Debug)]
struct BufferPoolInner {
    /// Lock-free queue containing available buffers
    pool: ArrayQueue<BufferData>,
    /// Size of each buffer in bytes
    buffer_size: usize,
    /// Maximum number of buffers in the pool
    capacity: usize,
    /// Total number of buffers allocated (including those outside the pool)
    total_allocated: AtomicUsize,
    /// Number of successful buffer acquisitions
    acquisitions: AtomicUsize,
    /// Number of buffer releases
    releases: AtomicUsize,
    /// Number of buffers allocated outside the pool (due to exhaustion)
    overflow_allocations: AtomicUsize,
}

/// Thread-safe buffer pool for high-performance I/O operations
#[derive(Debug)]
pub struct BufferPool {
    inner: Arc<BufferPoolInner>,
}

/// Internal buffer data
#[derive(Debug)]
struct BufferData {
    data: Box<[u8]>,
    #[cfg(debug_assertions)]
    is_poisoned: bool,
}

/// A buffer that can be acquired from the pool
pub struct Buffer {
    data: Option<BufferData>,
    pool_inner: Arc<BufferPoolInner>,
}

impl BufferPool {
    /// Creates a new `BufferPool` with the specified capacity and buffer size.
    ///
    /// # Arguments
    /// * `capacity` - Maximum number of buffers to keep in the pool (1 to 256)
    /// * `buffer_size` - Size of each buffer in bytes (must be page-aligned)
    ///
    /// # Errors
    /// Returns an error if:
    /// - Capacity is 0 or exceeds `MAX_POOL_CAPACITY`
    /// - Buffer size is 0, exceeds `MAX_BUFFER_SIZE`, or is not page-aligned
    /// - Total memory usage would exceed `MAX_TOTAL_MEMORY`
    ///
    /// # Panics
    /// This function contains debug assertions that will panic in debug builds if
    /// preconditions are violated, even though those same conditions return errors
    /// in release builds.
    pub fn new(capacity: usize, buffer_size: usize) -> Result<Self, BufferPoolError> {
        // Validate parameters for runtime errors
        if capacity == 0 || capacity > MAX_POOL_CAPACITY {
            return Err(BufferPoolError::InvalidCapacity);
        }
        if buffer_size == 0 || buffer_size > MAX_BUFFER_SIZE || buffer_size % PAGE_SIZE != 0 {
            return Err(BufferPoolError::InvalidBufferSize);
        }

        // Check for overflow and memory limits
        let total_memory = capacity
            .checked_mul(buffer_size)
            .ok_or(BufferPoolError::MemoryLimitExceeded)?;
        if total_memory > MAX_TOTAL_MEMORY {
            return Err(BufferPoolError::MemoryLimitExceeded);
        }

        // TIGER STYLE: Assert all preconditions for debug builds
        #[cfg(debug_assertions)]
        {
            assert!(capacity > 0, "Pool capacity must be positive");
            assert!(
                capacity <= MAX_POOL_CAPACITY,
                "Pool capacity {capacity} exceeds maximum {MAX_POOL_CAPACITY}"
            );
            assert!(buffer_size > 0, "Buffer size must be positive");
            assert!(
                buffer_size <= MAX_BUFFER_SIZE,
                "Buffer size {buffer_size} exceeds maximum {MAX_BUFFER_SIZE}"
            );
            assert!(
                buffer_size % PAGE_SIZE == 0,
                "Buffer size {buffer_size} must be aligned to page boundaries ({PAGE_SIZE}B)"
            );
            assert!(
                total_memory <= MAX_TOTAL_MEMORY,
                "Total memory usage {total_memory} would exceed maximum {MAX_TOTAL_MEMORY}"
            );
        }

        let pool = ArrayQueue::new(capacity);

        // Pre-allocate all buffers
        for _ in 0..capacity {
            let buffer_data = vec![0u8; buffer_size].into_boxed_slice();
            let buffer_inner = BufferData {
                data: buffer_data,
                #[cfg(debug_assertions)]
                is_poisoned: false,
            };

            // This should never fail since we just created the queue with this capacity
            pool.push(buffer_inner)
                .map_err(|_| BufferPoolError::InvalidCapacity)?;
        }

        let inner = Arc::new(BufferPoolInner {
            pool,
            buffer_size,
            capacity,
            total_allocated: AtomicUsize::new(capacity),
            acquisitions: AtomicUsize::new(0),
            releases: AtomicUsize::new(0),
            overflow_allocations: AtomicUsize::new(0),
        });

        Ok(Self { inner })
    }

    /// Acquires a buffer from the pool.
    ///
    /// If the pool is empty, this method will allocate a new buffer to maintain
    /// performance (backpressure through allocation rather than blocking).
    ///
    /// # Returns
    /// A `Buffer` that will automatically return to the pool when dropped.
    ///
    /// # Panics
    /// Only panics in debug builds if a poisoned buffer is detected (use after release).
    #[must_use]
    pub fn acquire(&self) -> Buffer {
        self.inner.acquisitions.fetch_add(1, Ordering::Relaxed);

        let buffer_data = if let Some(buffer_data) = self.inner.pool.pop() {
            #[cfg(debug_assertions)]
            {
                assert!(
                    !buffer_data.is_poisoned,
                    "Acquired poisoned buffer - use after release detected"
                );
            }
            buffer_data
        } else {
            // Pool exhausted - allocate new buffer
            self.inner
                .overflow_allocations
                .fetch_add(1, Ordering::Relaxed);
            self.inner.total_allocated.fetch_add(1, Ordering::Relaxed);

            BufferData {
                data: vec![0u8; self.inner.buffer_size].into_boxed_slice(),
                #[cfg(debug_assertions)]
                is_poisoned: false,
            }
        };

        Buffer {
            data: Some(buffer_data),
            pool_inner: Arc::clone(&self.inner),
        }
    }

    /// Returns statistics about pool usage.
    #[must_use]
    pub fn stats(&self) -> BufferPoolStats {
        BufferPoolStats {
            capacity: self.inner.capacity,
            buffer_size: self.inner.buffer_size,
            available: self.inner.pool.len(),
            total_allocated: self.inner.total_allocated.load(Ordering::Relaxed),
            acquisitions: self.inner.acquisitions.load(Ordering::Relaxed),
            releases: self.inner.releases.load(Ordering::Relaxed),
            overflow_allocations: self.inner.overflow_allocations.load(Ordering::Relaxed),
        }
    }
}

impl Buffer {
    /// Returns a mutable reference to the buffer data.
    ///
    /// # Panics
    /// Panics if the buffer has already been released (used after release).
    #[allow(clippy::unwrap_used)] // Documented panic behavior
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        self.data.as_mut().unwrap().data.as_mut()
    }

    /// Returns an immutable reference to the buffer data.
    ///
    /// # Panics
    /// Panics if the buffer has already been released (used after release).
    #[must_use]
    #[allow(clippy::unwrap_used)] // Documented panic behavior
    pub fn as_slice(&self) -> &[u8] {
        self.data.as_ref().unwrap().data.as_ref()
    }

    /// Returns the size of the buffer in bytes.
    ///
    /// # Panics
    /// Panics if the buffer has already been released (used after release).
    #[must_use]
    #[allow(clippy::unwrap_used)] // Documented panic behavior
    pub fn len(&self) -> usize {
        self.data.as_ref().unwrap().data.len()
    }

    /// Returns true if the buffer is empty (length is 0).
    ///
    /// # Panics
    /// Panics if the buffer has already been released (used after release).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        if let Some(mut buffer_data) = self.data.take() {
            // Track the release
            self.pool_inner.releases.fetch_add(1, Ordering::Relaxed);

            // Zero the buffer for security
            buffer_data.data.fill(0);

            #[cfg(debug_assertions)]
            {
                buffer_data.is_poisoned = false;
            }

            // Try to return to pool, otherwise let it be deallocated
            if self.pool_inner.pool.push(buffer_data).is_err() {
                // Pool is full, buffer will be deallocated
                self.pool_inner
                    .total_allocated
                    .fetch_sub(1, Ordering::Relaxed);
            }
        }
    }
}

/// Statistics about buffer pool usage
#[derive(Debug, Clone)]
pub struct BufferPoolStats {
    /// Maximum number of buffers the pool can hold
    pub capacity: usize,
    /// Size of each buffer in bytes
    pub buffer_size: usize,
    /// Number of buffers currently available in the pool
    pub available: usize,
    /// Total number of buffers allocated (including those outside the pool)
    pub total_allocated: usize,
    /// Total number of buffer acquisitions
    pub acquisitions: usize,
    /// Total number of buffer releases
    pub releases: usize,
    /// Number of buffers allocated outside the pool due to exhaustion
    pub overflow_allocations: usize,
}

impl BufferPoolStats {
    /// Returns the current utilization ratio (0.0 to 1.0)
    #[must_use]
    pub fn utilization(&self) -> f64 {
        if self.capacity == 0 {
            return 0.0;
        }
        f64::from(u32::try_from(self.capacity - self.available).unwrap_or(u32::MAX))
            / f64::from(u32::try_from(self.capacity).unwrap_or(u32::MAX))
    }

    /// Returns the hit rate (buffers served from pool vs allocated)
    #[must_use]
    pub fn hit_rate(&self) -> f64 {
        if self.acquisitions == 0 {
            return 1.0;
        }
        f64::from(u32::try_from(self.acquisitions - self.overflow_allocations).unwrap_or(u32::MAX))
            / f64::from(u32::try_from(self.acquisitions).unwrap_or(u32::MAX))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    #[allow(clippy::expect_used)] // Acceptable in tests for cleaner test code
    fn test_basic_acquire_release_cycle() {
        let pool = BufferPool::new(4, PAGE_SIZE).expect("Failed to create pool");

        // Acquire a buffer
        let mut buffer = pool.acquire();
        assert_eq!(buffer.len(), PAGE_SIZE);

        // Write some data
        buffer.as_mut_slice()[0] = 42;
        assert_eq!(buffer.as_slice()[0], 42);

        // Check pool stats
        let stats = pool.stats();
        assert_eq!(stats.acquisitions, 1);
        assert_eq!(stats.available, 3); // 4 - 1 = 3 available

        // Buffer should be returned to pool on drop
        drop(buffer);

        // Small delay to ensure drop completes
        thread::sleep(std::time::Duration::from_millis(1));

        let stats = pool.stats();
        assert_eq!(stats.releases, 1);
        assert_eq!(stats.available, 4); // Back to full capacity
    }

    #[test]
    #[allow(clippy::expect_used)] // Acceptable in tests for cleaner test code
    fn test_concurrent_access() {
        let pool = Arc::new(BufferPool::new(8, PAGE_SIZE).expect("Failed to create pool"));
        let num_threads = 16;
        let operations_per_thread = 100;

        let handles: Vec<_> = (0..num_threads)
            .map(|thread_id| {
                let pool_clone = Arc::clone(&pool);
                thread::spawn(move || {
                    for i in 0..operations_per_thread {
                        let mut buffer = pool_clone.acquire();

                        // Write thread-specific data
                        #[allow(clippy::expect_used)] // Value is guaranteed to fit in u8 by modulo
                        let data =
                            u8::try_from((thread_id * 1000 + i) % 256).expect("value fits in u8");
                        buffer.as_mut_slice()[0] = data;

                        // Verify data integrity
                        assert_eq!(buffer.as_slice()[0], data);

                        // Simulate some work
                        thread::sleep(std::time::Duration::from_micros(1));

                        // Buffer automatically released on drop
                    }
                })
            })
            .collect();

        // Wait for all threads to complete
        for handle in handles {
            handle.join().expect("Thread panicked");
        }

        let stats = pool.stats();
        assert_eq!(
            stats.acquisitions,
            usize::try_from(num_threads * operations_per_thread)
                .expect("thread count fits in usize")
        );
        assert_eq!(
            stats.releases,
            usize::try_from(num_threads * operations_per_thread)
                .expect("thread count fits in usize")
        );
        // Pool should be back to full capacity
        assert_eq!(stats.available, 8);
    }

    #[test]
    #[allow(clippy::expect_used)] // Acceptable in tests for cleaner test code
    fn test_pool_exhaustion_and_backpressure() {
        let pool_capacity = 2;
        let pool = BufferPool::new(pool_capacity, PAGE_SIZE).expect("Failed to create pool");

        // Acquire more buffers than pool capacity
        let buffer1 = pool.acquire();
        let buffer2 = pool.acquire();
        let buffer3 = pool.acquire(); // This should trigger overflow allocation

        let stats = pool.stats();
        assert_eq!(stats.acquisitions, 3);
        assert_eq!(stats.available, 0); // Pool is empty
        assert_eq!(stats.overflow_allocations, 1); // One overflow allocation
        assert_eq!(stats.total_allocated, 3); // 2 in pool + 1 overflow

        // Release buffers
        drop(buffer1);
        drop(buffer2);
        drop(buffer3);

        // Give time for drops to complete
        thread::sleep(std::time::Duration::from_millis(1));

        let stats = pool.stats();
        assert_eq!(stats.releases, 3);
        assert_eq!(stats.available, 2); // Pool capacity restored
        assert_eq!(stats.total_allocated, 2); // Overflow buffer was deallocated
    }

    #[test]
    fn test_memory_limit_enforcement() {
        // Try to create a pool that would exceed memory limits
        // 20 buffers * 64MB = 1.28GB > 1GB limit
        let result = BufferPool::new(20, MAX_BUFFER_SIZE);
        assert!(matches!(result, Err(BufferPoolError::MemoryLimitExceeded)));
    }

    #[test]
    fn test_invalid_parameters() {
        // Zero capacity
        assert!(matches!(
            BufferPool::new(0, PAGE_SIZE),
            Err(BufferPoolError::InvalidCapacity)
        ));

        // Capacity too large
        assert!(matches!(
            BufferPool::new(MAX_POOL_CAPACITY + 1, PAGE_SIZE),
            Err(BufferPoolError::InvalidCapacity)
        ));

        // Zero buffer size
        assert!(matches!(
            BufferPool::new(1, 0),
            Err(BufferPoolError::InvalidBufferSize)
        ));

        // Buffer size too large
        assert!(matches!(
            BufferPool::new(1, MAX_BUFFER_SIZE + 1),
            Err(BufferPoolError::InvalidBufferSize)
        ));

        // Unaligned buffer size
        assert!(matches!(
            BufferPool::new(1, PAGE_SIZE + 1),
            Err(BufferPoolError::InvalidBufferSize)
        ));
    }

    #[test]
    #[allow(clippy::expect_used)] // Acceptable in tests for cleaner test code
    fn test_buffer_zeroing_on_release() {
        let pool = BufferPool::new(1, PAGE_SIZE).expect("Failed to create pool");

        {
            let mut buffer = pool.acquire();
            // Write some data
            buffer.as_mut_slice().fill(0xFF);
        } // Buffer released here

        // Acquire the same buffer
        let buffer = pool.acquire();
        // Data should be zeroed
        assert!(buffer.as_slice().iter().all(|&b| b == 0));
    }

    #[test]
    #[allow(clippy::expect_used)] // Acceptable in tests for cleaner test code
    fn test_stats_accuracy() {
        let pool = BufferPool::new(3, PAGE_SIZE * 2).expect("Failed to create pool");

        let initial_stats = pool.stats();
        assert_eq!(initial_stats.capacity, 3);
        assert_eq!(initial_stats.buffer_size, PAGE_SIZE * 2);
        assert_eq!(initial_stats.available, 3);
        assert_eq!(initial_stats.total_allocated, 3);
        assert_eq!(initial_stats.acquisitions, 0);
        assert_eq!(initial_stats.releases, 0);
        assert_eq!(initial_stats.overflow_allocations, 0);
        assert!((initial_stats.utilization() - 0.0).abs() < f64::EPSILON);
        assert!((initial_stats.hit_rate() - 1.0).abs() < f64::EPSILON);

        let buffer = pool.acquire();
        let stats = pool.stats();
        assert_eq!(stats.acquisitions, 1);
        assert_eq!(stats.available, 2);
        assert!(stats.utilization() > 0.0);

        drop(buffer);
        thread::sleep(std::time::Duration::from_millis(1));

        let final_stats = pool.stats();
        assert_eq!(final_stats.releases, 1);
        assert_eq!(final_stats.available, 3);
        assert!((final_stats.utilization() - 0.0).abs() < f64::EPSILON);
        assert!((final_stats.hit_rate() - 1.0).abs() < f64::EPSILON);
    }
}
