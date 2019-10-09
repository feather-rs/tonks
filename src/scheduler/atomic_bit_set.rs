use crossbeam::utils::Backoff;
use std::iter;
use std::mem::size_of;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// An atomic bit set with support for waiting on updates.
///
/// Concurrent reallocations are not supported. If capacity needs to be
/// expanded, unique access (`&mut`) is required.
pub struct AtomicBitSet {
    /// Flag containing the number of updates performed on this bit set
    /// since the last call to `wait_on_update`.
    flag: AtomicBool,
    /// Data in this bit set.
    data: Vec<AtomicUsize>,
}

impl AtomicBitSet {
    /// Creates a new `AtomicBitSet` with the
    /// given initial capacity.
    ///
    /// The capacity is rounded up to the nearest
    /// multiple of `size_of::<usize>()`.
    pub fn with_capacity(mut capacity: usize) -> Self {
        let size_of_usize = size_of::<usize>();

        let remainder = capacity % size_of_usize;
        if remainder > 0 {
            capacity += size_of_usize - remainder;
        }

        Self {
            flag: AtomicBool::new(false),
            data: iter::repeat_with(|| AtomicUsize::new(0))
                .take(capacity)
                .collect(),
        }
    }

    /// Atomically the given bit to this bit set.
    /// If `notify` is set to true, notifies the thread
    /// currently waiting on updates.
    ///
    /// # Panics
    /// Panics if `index >= capacity`.
    pub fn insert(&self, index: usize, notify: bool) {
        let value = &self.data[usize_index(index)];

        value.fetch_or(1 << index_in_usize(index), Ordering::AcqRel);

        if notify {
            self.notify();
        }
    }

    /// Atomically removes the given bit from this bit set.
    /// If `notify` is set to true, notifies the thread
    /// currently waiting on updates.
    ///
    /// # Panics
    /// Panics if `index >= capacity`.
    pub fn remove(&self, index: usize, notify: bool) {
        let value = &self.data[usize_index(index)];

        value.fetch_and(!(1 << index_in_usize(index)), Ordering::AcqRel);

        if notify {
            self.notify();
        }
    }

    /// Returns whether the given bit is contained within this
    /// set.
    pub fn contains(&self, index: usize) -> bool {
        let mut value = self.data[usize_index(index)].load(Ordering::Acquire);

        value >>= index_in_usize(index);

        match value & 0x01 {
            0 => false,
            1 => true,
            _ => unreachable!(),
        }
    }

    /// Clears all bits in this bit set.
    pub fn clear(&self) {
        for value in &self.data {
            value.store(0, Ordering::Release);
        }
    }

    /// Waits on an update to be performed on this bit set.
    ///
    /// After returning, it is possible multiple updates were performed.
    /// Future calls to `wait_on_update`, if there are no future updates,
    /// will block indefinitely, so callers must account for multiple
    /// updates.
    ///
    /// Updates only notify this thread if the `notify` parameter
    /// is set to true.
    ///
    /// This operation should not be performed by more than one
    /// thread concurrently.
    pub fn wait_on_update(&self) {
        let backoff = Backoff::new();
        while !self.flag.compare_and_swap(true, false, Ordering::Acquire) {
            backoff.snooze();
        }

        // Reset flag
    }

    fn notify(&self) {
        self.flag.store(true, Ordering::Release);
    }
}

fn usize_index(index: usize) -> usize {
    index / size_of::<usize>()
}

fn index_in_usize(index: usize) -> usize {
    index % size_of::<usize>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_basic() {
        let set = AtomicBitSet::with_capacity(64);

        for i in 0..64 {
            assert!(!set.contains(i));
            set.insert(i, false);
            assert!(set.contains(i));

            if i % 3 == 0 {
                set.remove(i, false);
            }
        }

        for i in 0..64 {
            assert_eq!(set.contains(i), i % 3 != 0);
        }

        set.clear();

        for i in 0..64 {
            assert!(!set.contains(i));
        }
    }

    #[test]
    fn test_wait_on_update() {
        let set = Arc::new(AtomicBitSet::with_capacity(64));

        set.insert(0, true);
        set.wait_on_update();
        assert!(set.contains(0));
        set.remove(0, false);
        assert!(!set.contains(0));

        let set_clone = Arc::clone(&set);
        std::thread::spawn(move || {
            set_clone.insert(0, true);
        });

        set.wait_on_update();
        assert!(set.contains(0));
    }

    #[test]
    fn check_traits() {
        static_assertions::assert_impl_all!(AtomicBitSet: Send, Sync);
    }
}
