//! [`StringPool`]: interning of equal text into one shared allocation.

use std::collections::HashSet;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

/// A thread-safe set of shared strings: equal text always comes back as the same [`Arc<str>`].
///
/// Interning lets code compare strings by pointer ([`Arc::ptr_eq`]) and keep one copy of text
/// that many objects repeat, such as identifiers and property names. Entries stay in the pool
/// until [`StringPool::garbage_collect`] drops the ones nobody else holds.
///
/// All methods take `&self` and lock an internal mutex, so the pool is `Send + Sync` and can be
/// shared across threads. They allocate and lock: do not call them on the audio thread.
///
/// ```
/// use std::sync::Arc;
/// use syrp_strings::StringPool;
/// let pool = StringPool::new();
/// let a = pool.get("gain");
/// let b = pool.get(&String::from("gain"));
/// assert!(Arc::ptr_eq(&a, &b));
/// assert_eq!(pool.len(), 1);
/// ```
#[derive(Default)]
pub struct StringPool {
    entries: Mutex<HashSet<Arc<str>>>,
}

impl StringPool {
    /// Creates an empty pool.
    ///
    /// ```
    /// assert!(syrp_strings::StringPool::new().is_empty());
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// The process-wide pool, created empty on first use.
    ///
    /// ```
    /// use std::sync::Arc;
    /// use syrp_strings::StringPool;
    /// let a = StringPool::global().get("shared-id");
    /// assert!(Arc::ptr_eq(&a, &StringPool::global().get("shared-id")));
    /// ```
    #[doc(alias = "getGlobalPool")]
    pub fn global() -> &'static StringPool {
        static GLOBAL: OnceLock<StringPool> = OnceLock::new();
        GLOBAL.get_or_init(StringPool::new)
    }

    /// Returns the pooled copy of `text`, adding it if the pool has no equal entry.
    ///
    /// Two calls with equal text return clones of the same [`Arc`], so [`Arc::ptr_eq`] holds
    /// between them for as long as the entry stays in the pool.
    ///
    /// ```
    /// use std::sync::Arc;
    /// let pool = syrp_strings::StringPool::new();
    /// assert!(Arc::ptr_eq(&pool.get("x"), &pool.get("x")));
    /// assert!(!Arc::ptr_eq(&pool.get("x"), &pool.get("y")));
    /// ```
    #[doc(alias = "getPooledString")]
    pub fn get(&self, text: &str) -> Arc<str> {
        let mut entries = self.lock();
        if let Some(existing) = entries.get(text) {
            return Arc::clone(existing);
        }
        let entry: Arc<str> = Arc::from(text);
        entries.insert(Arc::clone(&entry));
        entry
    }

    /// The number of distinct strings in the pool.
    ///
    /// ```
    /// let pool = syrp_strings::StringPool::new();
    /// pool.get("a");
    /// pool.get("a");
    /// pool.get("b");
    /// assert_eq!(pool.len(), 2);
    /// ```
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    /// Whether the pool holds no strings.
    ///
    /// ```
    /// let pool = syrp_strings::StringPool::new();
    /// assert!(pool.is_empty());
    /// let _id = pool.get("id");
    /// assert!(!pool.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.lock().is_empty()
    }

    /// Removes every entry that only the pool holds, freeing its text.
    ///
    /// Strings still referenced outside the pool stay, so later [`StringPool::get`] calls keep
    /// returning the same allocation for them.
    ///
    /// ```
    /// let pool = syrp_strings::StringPool::new();
    /// let kept = pool.get("kept");
    /// pool.get("dropped");
    /// pool.garbage_collect();
    /// assert_eq!(pool.len(), 1);
    /// # drop(kept);
    /// ```
    #[doc(alias = "garbageCollect")]
    pub fn garbage_collect(&self) {
        self.lock().retain(|entry| Arc::strong_count(entry) > 1);
    }

    fn lock(&self) -> MutexGuard<'_, HashSet<Arc<str>>> {
        // The set is never left half-updated by a panic, so a poisoned lock is still usable.
        self.entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl fmt::Debug for StringPool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StringPool")
            .field("len", &self.len())
            .finish()
    }
}
