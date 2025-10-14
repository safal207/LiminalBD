use anyhow::{Context, Result};
use std::fs;

use crate::wal::{list_segments, wal_path, Offset, Store};

pub fn gc_compact(store: &Store, since: Offset) -> Result<()> {
    let current = store.current_segment();
    let data_dir = store.wal_dir().to_path_buf();
    let segments = list_segments(&data_dir)?;
    for segment in segments {
        if segment < since.segment && segment != current {
            let path = wal_path(&data_dir, segment);
            if path.exists() {
                fs::remove_file(&path)
                    .with_context(|| format!("failed to remove old wal segment {:?}", path))?;
            }
        }
    }
    Ok(())
}
