use chrono::{DateTime, Utc};

use crate::zone::LocalZone;

/// Parse an RFC 3339 timestamp into UTC. Panics on bad input (test helper).
pub fn utc(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .expect("valid RFC 3339 timestamp")
        .with_timezone(&Utc)
}

/// Build a fixed IANA zone for tests. Panics on unknown names.
pub fn tz(name: &str) -> Box<dyn LocalZone> {
    Box::new(name.parse::<chrono_tz::Tz>().expect("valid IANA zone name"))
}

/// In-memory `Fs` for tests. Directories are implicit; only files are stored.
#[derive(Default)]
pub struct MemFs {
    pub files: std::cell::RefCell<std::collections::BTreeMap<std::path::PathBuf, Vec<u8>>>,
    /// When true, `write_sync`/`rename`/`copy`/`append` fail with `StorageFull`.
    pub fail_writes: std::cell::Cell<bool>,
    /// When true, only `rename` fails (with `StorageFull`).
    pub fail_renames: std::cell::Cell<bool>,
    /// Reads of this path fail with `PermissionDenied`.
    pub fail_reads_of: std::cell::RefCell<Option<std::path::PathBuf>>,
    /// When true, only `write_sync` fails (with `StorageFull`).
    pub fail_write_syncs: std::cell::Cell<bool>,
    /// When true, `list` fails with `PermissionDenied`.
    pub fail_lists: std::cell::Cell<bool>,
    /// When true, `exists` answers "maybe" (like `RealFs` on a permission error): true.
    pub exists_unknown: std::cell::Cell<bool>,
}

impl MemFs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn put(&self, p: impl Into<std::path::PathBuf>, b: &[u8]) {
        self.files.borrow_mut().insert(p.into(), b.to_vec());
    }

    pub fn get(&self, p: &std::path::Path) -> Option<Vec<u8>> {
        self.files.borrow().get(p).cloned()
    }

    pub fn names(&self) -> Vec<std::path::PathBuf> {
        self.files.borrow().keys().cloned().collect()
    }

    fn check_write(&self) -> std::io::Result<()> {
        if self.fail_writes.get() {
            Err(std::io::ErrorKind::StorageFull.into())
        } else {
            Ok(())
        }
    }
}

impl crate::storage::Fs for MemFs {
    fn read(&self, p: &std::path::Path) -> std::io::Result<Vec<u8>> {
        if self.fail_reads_of.borrow().as_deref() == Some(p) {
            return Err(std::io::ErrorKind::PermissionDenied.into());
        }
        self.get(p)
            .ok_or_else(|| std::io::ErrorKind::NotFound.into())
    }

    fn write_sync(&self, p: &std::path::Path, b: &[u8]) -> std::io::Result<()> {
        self.check_write()?;
        if self.fail_write_syncs.get() {
            return Err(std::io::ErrorKind::StorageFull.into());
        }
        self.put(p, b);
        Ok(())
    }

    fn rename(&self, from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
        self.check_write()?;
        if self.fail_renames.get() {
            return Err(std::io::ErrorKind::StorageFull.into());
        }
        let mut files = self.files.borrow_mut();
        let b = files
            .remove(from)
            .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))?;
        files.insert(to.to_path_buf(), b);
        Ok(())
    }

    fn copy(&self, from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
        self.check_write()?;
        let mut files = self.files.borrow_mut();
        let b = files
            .get(from)
            .cloned()
            .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))?;
        files.insert(to.to_path_buf(), b);
        Ok(())
    }

    fn exists(&self, p: &std::path::Path) -> bool {
        self.exists_unknown.get() || self.files.borrow().contains_key(p)
    }

    fn create_dir_all(&self, _p: &std::path::Path) -> std::io::Result<()> {
        Ok(())
    }

    fn list(&self, dir: &std::path::Path) -> std::io::Result<Vec<std::path::PathBuf>> {
        if self.fail_lists.get() {
            return Err(std::io::ErrorKind::PermissionDenied.into());
        }
        Ok(self
            .files
            .borrow()
            .keys()
            .filter(|k| k.parent() == Some(dir))
            .cloned()
            .collect())
    }

    fn remove(&self, p: &std::path::Path) -> std::io::Result<()> {
        self.files
            .borrow_mut()
            .remove(p)
            .map(|_| ())
            .ok_or_else(|| std::io::ErrorKind::NotFound.into())
    }

    fn append(&self, p: &std::path::Path, b: &[u8]) -> std::io::Result<()> {
        self.check_write()?;
        self.files
            .borrow_mut()
            .entry(p.to_path_buf())
            .or_default()
            .extend_from_slice(b);
        Ok(())
    }
}
