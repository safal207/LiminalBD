use std::collections::BTreeMap;
use std::fs::{create_dir_all, File, OpenOptions};
use std::io::{ErrorKind, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use parking_lot::Mutex;
use sled::{Config as SledConfig, Db};

use liminal_sal::{Sal, SalOpts};

const DEFAULT_WAL_FILE: &str = "events.wal";

pub struct SledSal {
    db: Db,
    wal_path: PathBuf,
    wal: Mutex<WalWriter>,
}

impl SledSal {
    pub fn open(path_fragment: &str, opts: SalOpts) -> Result<Self> {
        let path = normalize_path(path_fragment);
        if opts.create_if_missing {
            create_dir_all(&path)
                .with_context(|| format!("failed to create sled path at {path:?}"))?;
        }
        let db = SledConfig::default().path(&path).open()?;
        let wal_path = path.join(DEFAULT_WAL_FILE);
        let writer = WalWriter::open(&wal_path)?;
        Ok(SledSal {
            db,
            wal_path,
            wal: Mutex::new(writer),
        })
    }
}

impl Sal for SledSal {
    fn kv_get(&self, ns: &str, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let tree = self.db.open_tree(ns)?;
        Ok(tree.get(key)?.map(|ivec| ivec.to_vec()))
    }

    fn kv_put(&self, ns: &str, key: &[u8], val: &[u8]) -> Result<()> {
        let tree = self.db.open_tree(ns)?;
        tree.insert(key, val)?;
        tree.flush()?;
        Ok(())
    }

    fn kv_del(&self, ns: &str, key: &[u8]) -> Result<()> {
        let tree = self.db.open_tree(ns)?;
        tree.remove(key)?;
        tree.flush()?;
        Ok(())
    }

    fn scan_prefix(
        &self,
        ns: &str,
        prefix: &[u8],
        limit: usize,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let tree = self.db.open_tree(ns)?;
        let iter = tree.scan_prefix(prefix);
        let mut out = Vec::new();
        for result in iter.take(limit) {
            let (k, v) = result?;
            out.push((k.to_vec(), v.to_vec()));
        }
        Ok(out)
    }

    fn wal_append(&self, record: &[u8]) -> Result<u64> {
        let mut guard = self.wal.lock();
        guard.append(record)
    }

    fn wal_read(&self, offset: u64, max: u64) -> Result<BTreeMap<u64, Vec<u8>>> {
        let mut reader = WalReader::open(&self.wal_path)?;
        reader.read_from(offset, max)
    }

    fn flush(&self) -> Result<()> {
        self.db.flush()?;
        Ok(())
    }
}

struct WalWriter {
    file: File,
    next_offset: u64,
}

impl WalWriter {
    fn open(path: &Path) -> Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(path)
            .with_context(|| format!("failed to open wal file {path:?}"))?;
        let next_offset = file.metadata()?.len();
        Ok(WalWriter { file, next_offset })
    }

    fn append(&mut self, record: &[u8]) -> Result<u64> {
        let offset = self.next_offset;
        let len = u32::try_from(record.len()).map_err(|_| anyhow!("record too large"))?;
        self.file.write_all(&len.to_le_bytes())?;
        self.file.write_all(record)?;
        self.file.flush()?;
        self.next_offset += 4 + record.len() as u64;
        Ok(offset)
    }
}

struct WalReader {
    file: File,
}

impl WalReader {
    fn open(path: &Path) -> Result<Self> {
        let result = OpenOptions::new().read(true).open(path);
        match result {
            Ok(file) => Ok(WalReader { file }),
            Err(err) if err.kind() == ErrorKind::NotFound => {
                File::create(path)?;
                let file = OpenOptions::new().read(true).open(path)?;
                Ok(WalReader { file })
            }
            Err(err) => Err(err.into()),
        }
    }

    fn read_from(&mut self, offset: u64, max: u64) -> Result<BTreeMap<u64, Vec<u8>>> {
        self.file.seek(SeekFrom::Start(offset))?;
        let mut out = BTreeMap::new();
        let mut current_offset = offset;
        let mut read = 0;
        loop {
            if read >= max {
                break;
            }
            let mut len_buf = [0u8; 4];
            match self.file.read_exact(&mut len_buf) {
                Ok(()) => {}
                Err(err) if err.kind() == ErrorKind::UnexpectedEof => break,
                Err(err) => return Err(err.into()),
            }
            let len = u32::from_le_bytes(len_buf) as usize;
            let mut buf = vec![0u8; len];
            self.file.read_exact(&mut buf)?;
            out.insert(current_offset, buf);
            current_offset += 4 + len as u64;
            read += 1;
        }
        Ok(out)
    }
}

fn normalize_path(fragment: &str) -> PathBuf {
    let trimmed = fragment.trim();
    if trimmed.is_empty() {
        return PathBuf::from(".");
    }
    PathBuf::from(trimmed)
}
