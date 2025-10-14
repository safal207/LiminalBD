use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result};
use parking_lot::Mutex;

use liminal_core::{ClusterField, EventDelta, Journal};

use crate::codec::encode_delta;
use crate::gc::gc_compact;
use crate::snapshot::{create_snapshot, load_snapshot, ClusterFieldSeed};
use crate::wal::{list_segments, Offset, Store, StoreManifest, WalStream};

#[derive(Debug, Clone)]
pub struct SnapshotInfo {
    pub id: u64,
    pub path: PathBuf,
    pub offset: Offset,
    pub size_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct StoreStats {
    pub current_segment: u64,
    pub current_position: u64,
    pub wal_segments: Vec<u64>,
    pub last_snapshot: Option<u64>,
    pub delta_since_snapshot: u64,
}

pub struct DiskJournal {
    store: Mutex<Store>,
    delta_counter: AtomicU64,
    last_snapshot_id: AtomicU64,
}

impl DiskJournal {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let store = Store::open(path)?;
        let last_snapshot_id = store.list_snapshots()?.into_iter().max().unwrap_or(0);
        Ok(DiskJournal {
            store: Mutex::new(store),
            delta_counter: AtomicU64::new(0),
            last_snapshot_id: AtomicU64::new(last_snapshot_id),
        })
    }

    pub fn end_offset(&self) -> Offset {
        let store = self.store.lock();
        store.end_offset()
    }

    pub fn stream_from(&self, offset: Offset) -> Result<WalStream> {
        let store = self.store.lock();
        let stream = store.stream_from(offset)?;
        Ok(stream)
    }

    pub fn load_latest_snapshot(&self) -> Result<Option<(ClusterFieldSeed, Offset)>> {
        let store = self.store.lock();
        let manifest = match store.read_manifest()? {
            Some(manifest) => manifest,
            None => return Ok(None),
        };
        let Some(name) = &manifest.last_snapshot else {
            return Ok(None);
        };
        let path = store.snap_dir().join(name);
        let bytes = fs::read(&path).with_context(|| format!("failed to read snapshot {path:?}"))?;
        let seed = load_snapshot(&bytes)?;
        let offset = Offset {
            segment: manifest.wal_segment,
            position: manifest.wal_position,
        };
        if let Some(stem) = Path::new(name)
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.parse::<u64>().ok())
        {
            self.last_snapshot_id.store(stem, Ordering::Relaxed);
        }
        self.delta_counter.store(0, Ordering::Relaxed);
        Ok(Some((seed, offset)))
    }

    pub fn write_snapshot(&self, cluster: &ClusterField) -> Result<SnapshotInfo> {
        let store = self.store.lock();
        let snapshot_bytes = create_snapshot(cluster)?;
        let offset = store.end_offset();
        let id = store.next_snapshot_id()?;
        let path = store.snapshot_path(id);
        fs::write(&path, &snapshot_bytes)
            .with_context(|| format!("failed to write snapshot {path:?}"))?;
        let manifest = StoreManifest {
            last_snapshot: Some(path.file_name().unwrap().to_string_lossy().to_string()),
            wal_segment: offset.segment,
            wal_position: offset.position,
        };
        store.write_manifest(&manifest)?;
        self.delta_counter.store(0, Ordering::Relaxed);
        self.last_snapshot_id.store(id, Ordering::Relaxed);
        Ok(SnapshotInfo {
            id,
            path,
            offset,
            size_bytes: snapshot_bytes.len() as u64,
        })
    }

    pub fn run_gc(&self, since: Offset) -> Result<()> {
        let store = self.store.lock();
        gc_compact(&store, since)
    }

    pub fn delta_since_snapshot(&self) -> u64 {
        self.delta_counter.load(Ordering::Relaxed)
    }

    pub fn last_snapshot_id(&self) -> Option<u64> {
        match self.last_snapshot_id.load(Ordering::Relaxed) {
            0 => None,
            id => Some(id),
        }
    }

    pub fn stats(&self) -> Result<StoreStats> {
        let store = self.store.lock();
        let mut segments = list_segments(store.wal_dir())?;
        segments.sort_unstable();
        let end = store.end_offset();
        Ok(StoreStats {
            current_segment: end.segment,
            current_position: end.position,
            wal_segments: segments,
            last_snapshot: self.last_snapshot_id(),
            delta_since_snapshot: self.delta_since_snapshot(),
        })
    }
}

impl Journal for DiskJournal {
    fn append_delta(&self, delta: &EventDelta) {
        match encode_delta(delta) {
            Ok(bytes) => {
                let result = if let Some(mut guard) = self.store.try_lock() {
                    guard.append(&bytes)
                } else {
                    let mut guard = self.store.lock();
                    guard.append(&bytes)
                };
                if let Err(err) = result {
                    eprintln!("journal append error: {err}");
                    return;
                }
                self.delta_counter.fetch_add(1, Ordering::Relaxed);
            }
            Err(err) => {
                eprintln!("journal encode error: {err}");
            }
        }
    }
}
