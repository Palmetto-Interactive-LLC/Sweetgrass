//! Short-TTL in-memory cache for the beads ledger (`/api/beads` responses).
//!
//! Reading the ledger shells out to `bd list --all --json` per request. For
//! repos with 150+ beads, the UI's rapid polls (SSE-triggered refetches,
//! multiple views mounting at once) would each spawn a fresh `bd` process.
//! This cache keeps the most recent successful response per project path for
//! a short TTL (default 2s), so bursts of requests are served from memory,
//! while the file-watcher and every bd write path invalidate the entry the
//! moment the underlying data changes.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

/// Default time-to-live for cached ledger responses.
const DEFAULT_TTL: Duration = Duration::from_secs(2);

/// Environment variable overriding the TTL, in milliseconds.
/// `0` disables caching entirely (every request shells out to bd again).
const TTL_ENV_VAR: &str = "SWEETGRASS_BD_CACHE_TTL_MS";

/// Process-wide cache instance used by the route handlers.
static GLOBAL: LazyLock<BdListCache> = LazyLock::new(|| BdListCache::new(ttl_from_env()));

/// Returns the process-wide ledger cache.
pub fn global() -> &'static BdListCache {
    &GLOBAL
}

/// Reads the TTL override from the environment, falling back to the default.
fn ttl_from_env() -> Duration {
    std::env::var(TTL_ENV_VAR)
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map(Duration::from_millis)
        .unwrap_or(DEFAULT_TTL)
}

/// A single cached response.
struct Entry {
    stored_at: Instant,
    value: Arc<serde_json::Value>,
}

/// TTL cache of `/api/beads` response bodies keyed by canonical project path.
pub struct BdListCache {
    ttl: Duration,
    entries: Mutex<HashMap<PathBuf, Entry>>,
}

impl BdListCache {
    /// Creates a cache with the given TTL. A zero TTL disables caching.
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// Canonical cache key for a project path, so `/p`, `/p/.` and symlinked
    /// spellings share one entry. Falls back to the literal path when the
    /// directory can't be canonicalized (e.g. it doesn't exist).
    fn key(project_path: &Path) -> PathBuf {
        project_path
            .canonicalize()
            .unwrap_or_else(|_| project_path.to_path_buf())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<PathBuf, Entry>> {
        // A poisoned lock only means another thread panicked mid-insert;
        // the map itself is still usable, so recover instead of cascading.
        self.entries.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Returns the cached response for a project if it is still fresh.
    /// Stale entries are evicted on the way out.
    pub fn get(&self, project_path: &Path) -> Option<Arc<serde_json::Value>> {
        let key = Self::key(project_path);
        let mut entries = self.lock();
        match entries.get(&key) {
            Some(entry) if entry.stored_at.elapsed() < self.ttl => {
                Some(Arc::clone(&entry.value))
            }
            Some(_) => {
                entries.remove(&key);
                None
            }
            None => None,
        }
    }

    /// Stores a response for a project. No-op when caching is disabled.
    pub fn put(&self, project_path: &Path, value: Arc<serde_json::Value>) {
        if self.ttl.is_zero() {
            return;
        }
        let key = Self::key(project_path);
        self.lock().insert(
            key,
            Entry {
                stored_at: Instant::now(),
                value,
            },
        );
    }

    /// Drops the cached response for a project. Called by the file-watcher on
    /// `.beads` changes and by every bd write path so the next read is fresh.
    pub fn invalidate(&self, project_path: &Path) {
        let key = Self::key(project_path);
        self.lock().remove(&key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(n: u64) -> Arc<serde_json::Value> {
        Arc::new(serde_json::json!({ "beads": [n] }))
    }

    #[test]
    fn fresh_entry_is_served() {
        let cache = BdListCache::new(Duration::from_secs(60));
        let dir = tempfile::tempdir().unwrap();
        cache.put(dir.path(), value(1));
        let hit = cache.get(dir.path()).expect("fresh entry should hit");
        assert_eq!(*hit, *value(1));
    }

    #[test]
    fn miss_when_nothing_cached() {
        let cache = BdListCache::new(Duration::from_secs(60));
        let dir = tempfile::tempdir().unwrap();
        assert!(cache.get(dir.path()).is_none());
    }

    #[test]
    fn expired_entry_is_evicted() {
        let cache = BdListCache::new(Duration::from_millis(20));
        let dir = tempfile::tempdir().unwrap();
        cache.put(dir.path(), value(1));
        std::thread::sleep(Duration::from_millis(40));
        assert!(cache.get(dir.path()).is_none(), "stale entry must not hit");
        // The stale entry was evicted, not just skipped
        assert!(cache.lock().is_empty());
    }

    #[test]
    fn invalidate_removes_entry() {
        let cache = BdListCache::new(Duration::from_secs(60));
        let dir = tempfile::tempdir().unwrap();
        cache.put(dir.path(), value(1));
        cache.invalidate(dir.path());
        assert!(cache.get(dir.path()).is_none());
    }

    #[test]
    fn entries_are_keyed_per_project() {
        let cache = BdListCache::new(Duration::from_secs(60));
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        cache.put(a.path(), value(1));
        cache.put(b.path(), value(2));
        assert_eq!(*cache.get(a.path()).unwrap(), *value(1));
        assert_eq!(*cache.get(b.path()).unwrap(), *value(2));
        cache.invalidate(a.path());
        assert!(cache.get(a.path()).is_none());
        assert!(cache.get(b.path()).is_some(), "other projects unaffected");
    }

    #[test]
    fn zero_ttl_disables_caching() {
        let cache = BdListCache::new(Duration::ZERO);
        let dir = tempfile::tempdir().unwrap();
        cache.put(dir.path(), value(1));
        assert!(cache.get(dir.path()).is_none());
        assert!(cache.lock().is_empty(), "disabled cache stores nothing");
    }

    #[test]
    fn key_is_canonicalized_across_path_spellings() {
        let cache = BdListCache::new(Duration::from_secs(60));
        let dir = tempfile::tempdir().unwrap();
        // Store under the plain path, hit via a `.`-suffixed spelling.
        cache.put(dir.path(), value(1));
        assert!(cache.get(&dir.path().join(".")).is_some());
        // Invalidating via the alternate spelling clears the plain one too.
        cache.invalidate(&dir.path().join("."));
        assert!(cache.get(dir.path()).is_none());
    }

    #[test]
    fn newer_put_replaces_older_value() {
        let cache = BdListCache::new(Duration::from_secs(60));
        let dir = tempfile::tempdir().unwrap();
        cache.put(dir.path(), value(1));
        cache.put(dir.path(), value(2));
        assert_eq!(*cache.get(dir.path()).unwrap(), *value(2));
    }
}
