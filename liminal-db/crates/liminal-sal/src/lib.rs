use std::collections::BTreeMap;

use anyhow::Result;

/// Options passed to SAL implementations during initialization.
#[derive(Debug, Clone, Default)]
pub struct SalOpts {
    /// Open the underlying store in read-only mode. Implementations may ignore
    /// this flag when the backend does not support read-only access.
    pub read_only: bool,
    /// Ensure that the namespace roots exist. Backends that require
    /// provisioning may use this flag to create the physical storage on open.
    pub create_if_missing: bool,
}

/// Storage Abstraction Layer (SAL).
///
/// The trait is intentionally object-safe so that adapters can be loaded
/// dynamically based on the runtime URI scheme. Backends are responsible for
/// implementing all methods in a consistent manner.
pub trait Sal: Send + Sync {
    /// Fetch a value from the key-value namespace. Returns `Ok(None)` when the
    /// key does not exist.
    fn kv_get(&self, ns: &str, key: &[u8]) -> Result<Option<Vec<u8>>>;

    /// Insert or replace a value in the namespace.
    fn kv_put(&self, ns: &str, key: &[u8], val: &[u8]) -> Result<()>;

    /// Remove a value from the namespace. It is not an error if the key does
    /// not exist.
    fn kv_del(&self, ns: &str, key: &[u8]) -> Result<()>;

    /// Scan up to `limit` entries with the provided prefix.
    fn scan_prefix(&self, ns: &str, prefix: &[u8], limit: usize)
        -> Result<Vec<(Vec<u8>, Vec<u8>)>>;

    /// Append a record to the write-ahead log. The returned offset must be
    /// monotonically increasing.
    fn wal_append(&self, record: &[u8]) -> Result<u64>;

    /// Read up to `max` records starting from `offset`. The returned map is
    /// keyed by the absolute WAL offset, allowing consumers to continue from
    /// the last processed entry.
    fn wal_read(&self, offset: u64, max: u64) -> Result<BTreeMap<u64, Vec<u8>>>;

    /// Flush any buffered state to the durable backing store.
    fn flush(&self) -> Result<()> {
        Ok(())
    }
}
