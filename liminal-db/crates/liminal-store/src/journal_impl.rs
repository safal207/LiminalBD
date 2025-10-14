use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use parking_lot::Mutex;

use liminal_core::{ClusterField, EventDelta, Journal};

use crate::codec::encode_delta;
use crate::gc::gc_compact;
use crate::snapshot::{create_snapshot, load_snapshot, ClusterFieldSeed};
use crate::wal::{Offset, Store, StoreManifest, WalStream};

pub struct DiskJournal {
    store: Mutex<Store>,
}

impl DiskJournal {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let store = Store::open(path)?;
        Ok(DiskJournal {
            store: Mutex::new(store),
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
        Ok(Some((seed, offset)))
    }

    pub fn write_snapshot(&self, cluster: &ClusterField) -> Result<(PathBuf, Offset)> {
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
        Ok((path, offset))
    }

    pub fn run_gc(&self, since: Offset) -> Result<()> {
        let store = self.store.lock();
        gc_compact(&store, since)
    }
}

impl Journal for DiskJournal {
    fn append_delta(&self, delta: &EventDelta) {
        match encode_delta(delta) {
            Ok(bytes) => {
                if let Some(mut guard) = self.store.try_lock() {
                    if let Err(err) = guard.append(&bytes) {
                        eprintln!("journal append error: {err}");
                    }
                } else {
                    let mut guard = self.store.lock();
                    if let Err(err) = guard.append(&bytes) {
                        eprintln!("journal append error: {err}");
                    }
                }
            }
            Err(err) => {
                eprintln!("journal encode error: {err}");
            }
        }
    }
}
