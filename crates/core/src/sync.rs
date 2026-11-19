//! Locks that survive the panic of another thread.
//!
//! A `std::sync::Mutex` is poisoned when a thread panics while holding it, and every later `lock().unwrap()` panics
//! too: one failure in a background task then takes down the window thread, the pipeline and the KWin readers that
//! share the lock. The state guarded by the locks of this program (caches, window tables, last-seen values) stays
//! structurally valid after a panic (each critical section is one insert, remove or read), so the guard is taken
//! back and the program goes on; the panic itself has already been logged by the panic hook.

use std::sync::{Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};

/// `lock()` without the poison panic.
pub trait LockExt<T> {
    /// The guard of the mutex; a poisoned mutex gives its guard back instead of panicking.
    fn lock_recover(&self) -> MutexGuard<'_, T>;
}

impl<T> LockExt<T> for Mutex<T> {
    fn lock_recover(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// `read()` and `write()` without the poison panic.
pub trait RwLockExt<T> {
    /// A shared guard; a poisoned lock gives it back instead of panicking.
    fn read_recover(&self) -> RwLockReadGuard<'_, T>;
    /// An exclusive guard; a poisoned lock gives it back instead of panicking.
    fn write_recover(&self) -> RwLockWriteGuard<'_, T>;
}

impl<T> RwLockExt<T> for RwLock<T> {
    fn read_recover(&self) -> RwLockReadGuard<'_, T> {
        self.read().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
    fn write_recover(&self) -> RwLockWriteGuard<'_, T> {
        self.write().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn a_mutex_poisoned_by_a_panicking_thread_is_still_usable() {
        let shared = Arc::new(Mutex::new(vec![1]));
        let worker = Arc::clone(&shared);
        let result = std::thread::spawn(move || {
            let mut guard = worker.lock_recover();
            guard.push(2);
            panic!("a task fails while holding the lock");
        }).join();
        assert!(result.is_err());
        assert!(shared.is_poisoned(), "the premise: the lock is poisoned");
        let mut guard = shared.lock_recover();
        guard.push(3);
        assert_eq!(*guard, vec![1, 2, 3]);
    }

    #[test]
    fn a_poisoned_rwlock_is_still_readable_and_writable() {
        let shared = Arc::new(RwLock::new(1));
        let worker = Arc::clone(&shared);
        let _ = std::thread::spawn(move || {
            let _guard = worker.write_recover();
            panic!("a task fails while writing");
        }).join();
        assert!(shared.is_poisoned());
        *shared.write_recover() += 1;
        assert_eq!(*shared.read_recover(), 2);
    }
}
