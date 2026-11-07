use std::io::{self, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, NaiveDate, Utc};

use crate::model::{DATA_VERSION, Data, parse_data, to_json};
use crate::zone::LocalZone;

pub const BACKUP_KEEP: usize = 30;

/// Minimal file-system abstraction so storage logic is testable in memory.
pub trait Fs {
    fn read(&self, p: &Path) -> io::Result<Vec<u8>>;
    /// Create/truncate, write everything and fsync.
    fn write_sync(&self, p: &Path, b: &[u8]) -> io::Result<()>;
    /// Must replace an existing destination (also on Windows).
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    fn copy(&self, from: &Path, to: &Path) -> io::Result<()>;
    /// True when the path exists *or cannot be checked*: callers use it to avoid replacing a
    /// file, so "unknown" must never read as "free".
    fn exists(&self, p: &Path) -> bool;
    fn create_dir_all(&self, p: &Path) -> io::Result<()>;
    /// Direct children only; a missing directory yields an empty list.
    fn list(&self, dir: &Path) -> io::Result<Vec<PathBuf>>;
    fn remove(&self, p: &Path) -> io::Result<()>;
    fn append(&self, p: &Path, b: &[u8]) -> io::Result<()>;
}

pub struct RealFs;

/// A failed existence check (e.g. permission denied) counts as "exists".
fn exists_or_unknown(r: io::Result<bool>) -> bool {
    r.unwrap_or(true)
}

impl Fs for RealFs {
    fn read(&self, p: &Path) -> io::Result<Vec<u8>> {
        std::fs::read(p)
    }

    fn write_sync(&self, p: &Path, b: &[u8]) -> io::Result<()> {
        let mut f = std::fs::File::create(p)?;
        f.write_all(b)?;
        f.sync_all()
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        std::fs::rename(from, to)?;
        // Make the rename itself durable (best effort; directories cannot be opened like
        // this on Windows, where the rename is already durable enough).
        #[cfg(unix)]
        if let Some(dir) = to.parent()
            && let Ok(d) = std::fs::File::open(dir)
        {
            let _ = d.sync_all();
        }
        Ok(())
    }

    fn copy(&self, from: &Path, to: &Path) -> io::Result<()> {
        std::fs::copy(from, to).map(|_| ())
    }

    fn exists(&self, p: &Path) -> bool {
        exists_or_unknown(p.try_exists())
    }

    fn create_dir_all(&self, p: &Path) -> io::Result<()> {
        std::fs::create_dir_all(p)
    }

    fn list(&self, dir: &Path) -> io::Result<Vec<PathBuf>> {
        let rd = match std::fs::read_dir(dir) {
            Ok(rd) => rd,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };
        rd.map(|e| e.map(|e| e.path())).collect()
    }

    fn remove(&self, p: &Path) -> io::Result<()> {
        std::fs::remove_file(p)
    }

    fn append(&self, p: &Path, b: &[u8]) -> io::Result<()> {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(p)?;
        f.write_all(b)
    }
}

#[derive(Debug, Clone)]
pub struct Paths {
    pub dir: PathBuf,
}

impl Paths {
    pub fn data(&self) -> PathBuf {
        self.dir.join("data.json")
    }

    pub fn tmp(&self) -> PathBuf {
        self.dir.join("data.json.tmp")
    }

    pub fn backups(&self) -> PathBuf {
        self.dir.join("backups")
    }

    pub fn backup_for(&self, date: NaiveDate) -> PathBuf {
        self.backups()
            .join(format!("data-{}.json", date.format("%Y-%m-%d")))
    }

    /// `stamp` is `YYYYMMDD-HHMMSS` (optionally with a `-N` suffix).
    pub fn corrupt(&self, stamp: &str) -> PathBuf {
        self.dir.join(format!("data.corrupt-{stamp}.json"))
    }

    pub fn activity_log(&self) -> PathBuf {
        self.dir.join("activity.log")
    }
}

pub fn default_dir() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join("partimer"))
}

/// Write tmp (fsync) then replace `data.json`. The old file stays intact on any failure, and
/// no tmp file is left behind.
pub fn save_atomic(fs: &dyn Fs, paths: &Paths, data: &Data) -> io::Result<()> {
    fs.create_dir_all(&paths.dir)?;
    let tmp = paths.tmp();
    let result = fs
        .write_sync(&tmp, &to_json(data))
        .and_then(|_| fs.rename(&tmp, &paths.data()));
    if result.is_err() {
        let _ = fs.remove(&tmp);
    }
    result
}

#[derive(Debug, Clone, PartialEq)]
pub enum LoadNotice {
    RestoredCorrupt {
        backup_date: NaiveDate,
        corrupt_path: PathBuf,
    },
    RestoredMissing {
        backup_date: NaiveDate,
    },
    CorruptNoBackup {
        corrupt_path: PathBuf,
    },
    /// `data.json` exists but could not be read or moved aside (e.g. permission
    /// denied). It was left untouched; saving must stay blocked. `error` is the raw
    /// io error; the UI wording is the caller's.
    Unreadable {
        reason: UnreadableReason,
        error: String,
        backup_date: Option<NaiveDate>,
    },
    /// The data (from `data.json` or a backup) has a `version` newer than this app knows.
    /// It is shown read-only and never written; saving must stay blocked.
    NewerVersion {
        version: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnreadableReason {
    /// Reading `data.json` failed.
    ReadFailed,
    /// `data.json` is corrupt and could not be renamed aside (or no free name was certain).
    CannotKeepCorrupt,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Loaded {
    pub data: Data,
    pub notice: Option<LoadNotice>,
    pub write_back_error: Option<String>,
    /// True when the data file is unreadable or from a newer version: the caller must not save.
    pub save_blocked: bool,
    /// The backups folder was needed but could not be listed (raw io error).
    pub backup_error: Option<String>,
}

impl Loaded {
    fn new(data: Data) -> Self {
        Loaded {
            data,
            notice: None,
            write_back_error: None,
            save_blocked: false,
            backup_error: None,
        }
    }

    fn notice(mut self, notice: LoadNotice) -> Self {
        self.notice = Some(notice);
        self
    }

    fn blocked(mut self) -> Self {
        self.save_blocked = true;
        self
    }
}

/// Data from a newer app version must be shown read-only and never written back.
fn newer_version(data: &Data) -> Option<LoadNotice> {
    (data.version > DATA_VERSION).then_some(LoadNotice::NewerVersion {
        version: data.version,
    })
}

fn backup_date_of(path: &Path) -> Option<NaiveDate> {
    let name = path.file_name()?.to_str()?;
    let date = name.strip_prefix("data-")?.strip_suffix(".json")?;
    let parsed = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    // Reject non-canonical spellings such as `data-2026-1-5.json`.
    (parsed.format("%Y-%m-%d").to_string() == date).then_some(parsed)
}

/// Backups named exactly `data-YYYY-MM-DD.json`, newest first. A missing folder is empty.
fn list_backups(fs: &dyn Fs, paths: &Paths) -> io::Result<Vec<(NaiveDate, PathBuf)>> {
    let mut v: Vec<_> = fs
        .list(&paths.backups())?
        .into_iter()
        .filter_map(|p| backup_date_of(&p).map(|d| (d, p)))
        .collect();
    v.sort_by_key(|(d, _)| std::cmp::Reverse(*d));
    Ok(v)
}

/// Newest readable backup dated up to `today`; one dated later (the clock was ahead when it
/// was taken) is used only when nothing else is readable.
fn newest_good_backup(
    fs: &dyn Fs,
    paths: &Paths,
    today: NaiveDate,
) -> io::Result<Option<(NaiveDate, Data)>> {
    let (past, future): (Vec<_>, Vec<_>) = list_backups(fs, paths)?
        .into_iter()
        .partition(|(d, _)| *d <= today);
    Ok(past.into_iter().chain(future).find_map(|(d, p)| {
        let data = parse_data(&fs.read(&p).ok()?).ok()?;
        Some((d, data))
    }))
}

fn split_backup(b: Option<(NaiveDate, Data)>) -> (Data, Option<NaiveDate>) {
    match b {
        Some((d, data)) => (data, Some(d)),
        None => (Data::default(), None),
    }
}

/// How many `-N` suffixes to try before giving up on finding a certainly free name.
const CORRUPT_NAME_TRIES: u32 = 100;

/// A corrupt-file name that certainly does not exist yet, or `None` (e.g. existence cannot
/// be checked): the rename would then risk replacing an older corrupt file.
fn unique_corrupt_path(
    fs: &dyn Fs,
    paths: &Paths,
    zone: &dyn LocalZone,
    now: DateTime<Utc>,
) -> Option<PathBuf> {
    let stamp = zone.to_local(now).format("%Y%m%d-%H%M%S").to_string();
    std::iter::once(paths.corrupt(&stamp))
        .chain((1..=CORRUPT_NAME_TRIES).map(|n| paths.corrupt(&format!("{stamp}-{n}"))))
        .find(|candidate| !fs.exists(candidate))
}

/// Load `data.json`, recovering from corruption or loss using backups.
/// Never panics and never overwrites or deletes the original data file.
pub fn load(fs: &dyn Fs, paths: &Paths, zone: &dyn LocalZone, now: DateTime<Utc>) -> Loaded {
    let today = crate::zone::local_date(zone, now);
    // Backups are only looked at when needed; a listing error is reported, not swallowed.
    let mut backup_error = None;
    let mut backup = || match newest_good_backup(fs, paths, today) {
        Ok(b) => b,
        Err(e) => {
            backup_error = Some(e.to_string());
            None
        }
    };
    let restored = |data: Data, notice: LoadNotice| {
        if let Some(newer) = newer_version(&data) {
            return Loaded::new(data).notice(newer).blocked();
        }
        let write_back_error = save_atomic(fs, paths, &data).err().map(|e| e.to_string());
        Loaded {
            write_back_error,
            ..Loaded::new(data).notice(notice)
        }
    };
    let unreadable = |b: Option<(NaiveDate, Data)>, reason, error: String| {
        let (data, backup_date) = split_backup(b);
        let notice = LoadNotice::Unreadable {
            reason,
            error,
            backup_date,
        };
        Loaded::new(data).notice(notice).blocked()
    };

    let loaded = match fs.read(&paths.data()) {
        Ok(bytes) => match parse_data(&bytes) {
            Ok(data) => match newer_version(&data) {
                Some(notice) => Loaded::new(data).notice(notice).blocked(),
                None => Loaded::new(data),
            },
            Err(_) => {
                let b = backup();
                // The bad file cannot be moved aside: leave it alone and block saving, since
                // a later save would replace it.
                let moved = unique_corrupt_path(fs, paths, zone, now)
                    .ok_or_else(|| "no free name for the corrupt file".to_string())
                    .and_then(|to| {
                        fs.rename(&paths.data(), &to)
                            .map(|_| to)
                            .map_err(|e| e.to_string())
                    });
                match (moved, b) {
                    (Err(e), b) => unreadable(b, UnreadableReason::CannotKeepCorrupt, e),
                    (Ok(corrupt_path), Some((backup_date, data))) => restored(
                        data,
                        LoadNotice::RestoredCorrupt {
                            backup_date,
                            corrupt_path,
                        },
                    ),
                    (Ok(corrupt_path), None) => Loaded::new(Data::default())
                        .notice(LoadNotice::CorruptNoBackup { corrupt_path }),
                }
            }
        },
        Err(e) if e.kind() == io::ErrorKind::NotFound => match backup() {
            Some((backup_date, data)) => {
                restored(data, LoadNotice::RestoredMissing { backup_date })
            }
            None => Loaded::new(Data::default()),
        },
        Err(e) => unreadable(backup(), UnreadableReason::ReadFailed, e.to_string()),
    };
    Loaded {
        backup_error,
        ..loaded
    }
}

/// Copy `data.json` to today's backup (once per day) and keep the newest `BACKUP_KEEP`.
pub fn ensure_daily_backup(fs: &dyn Fs, paths: &Paths, date: NaiveDate) -> io::Result<bool> {
    let bytes = match fs.read(&paths.data()) {
        Ok(b) => b,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e),
    };
    let target = paths.backup_for(date);
    if fs.exists(&target) {
        return Ok(false);
    }
    // Never back up a file that does not parse: it would only displace good backups.
    if parse_data(&bytes).is_err() {
        return Ok(false);
    }
    fs.create_dir_all(&paths.backups())?;
    // Durable write to a temp name, then rename: a partial file never counts as a backup.
    let tmp = paths
        .backups()
        .join(format!("data-{}.json.tmp", date.format("%Y-%m-%d")));
    let result = fs
        .write_sync(&tmp, &bytes)
        .and_then(|_| fs.rename(&tmp, &target));
    if let Err(e) = result {
        let _ = fs.remove(&tmp);
        return Err(e);
    }
    // Pruning is best effort: the backup itself already succeeded, and a listing error
    // only means the old backups are kept until the next day.
    if let Ok(list) = list_backups(fs, paths) {
        for (_, p) in list.into_iter().skip(BACKUP_KEEP) {
            let _ = fs.remove(&p);
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{MemFs, tz, utc};

    const GOOD_A: &str = r#"{"clients":[{"id":"a","name":"A"}]}"#;
    const GOOD_B: &str = r#"{"clients":[{"id":"b","name":"B"}]}"#;

    fn paths() -> Paths {
        Paths {
            dir: PathBuf::from("/d/partimer"),
        }
    }

    fn d(m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, m, day).unwrap()
    }

    fn now() -> DateTime<Utc> {
        utc("2026-10-07T01:02:03Z")
    }

    fn run_load(fs: &MemFs) -> Loaded {
        load(fs, &paths(), tz("Asia/Taipei").as_ref(), now())
    }

    fn sample() -> Data {
        parse_data(GOOD_A.as_bytes()).unwrap()
    }

    #[test]
    fn save_then_load_roundtrip() {
        let fs = MemFs::new();
        save_atomic(&fs, &paths(), &sample()).unwrap();
        let l = run_load(&fs);
        assert_eq!(l.data, sample());
        assert_eq!(l.notice, None);
        assert!(!l.save_blocked);
    }

    #[test]
    fn newer_version_file_loads_read_only() {
        let fs = MemFs::new();
        let newer = br#"{"version":2,"clients":[{"id":"a","name":"A"}],"future":true}"#;
        fs.put(paths().data(), newer);
        let l = run_load(&fs);
        assert_eq!(l.data.clients.len(), 1);
        assert_eq!(l.notice, Some(LoadNotice::NewerVersion { version: 2 }));
        assert_eq!(l.write_back_error, None);
        assert!(l.save_blocked);
        assert_eq!(fs.get(&paths().data()).unwrap(), newer);
        assert!(!fs.exists(&paths().tmp()));
    }

    #[test]
    fn newer_version_backup_is_not_written_back() {
        let fs = MemFs::new();
        fs.put(
            paths().backup_for(d(10, 6)),
            br#"{"version":3,"clients":[{"id":"a","name":"A"}]}"#,
        );
        let l = run_load(&fs);
        assert_eq!(l.data.clients.len(), 1);
        assert_eq!(l.notice, Some(LoadNotice::NewerVersion { version: 3 }));
        assert!(l.save_blocked);
        assert!(!fs.exists(&paths().data()));
    }

    #[test]
    fn current_version_file_is_not_blocked() {
        let fs = MemFs::new();
        fs.put(paths().data(), br#"{"version":1}"#);
        let l = run_load(&fs);
        assert_eq!(l.notice, None);
        assert!(!l.save_blocked);
    }

    #[test]
    fn save_leaves_no_tmp() {
        let fs = MemFs::new();
        save_atomic(&fs, &paths(), &sample()).unwrap();
        assert!(!fs.exists(&paths().tmp()));
        assert!(fs.exists(&paths().data()));
    }

    #[test]
    fn save_failure_keeps_old_file() {
        let fs = MemFs::new();
        fs.put(paths().data(), GOOD_B.as_bytes());
        fs.fail_writes.set(true);
        assert!(save_atomic(&fs, &paths(), &sample()).is_err());
        assert_eq!(fs.get(&paths().data()).unwrap(), GOOD_B.as_bytes());
    }

    #[test]
    fn fresh_install_empty_no_notice() {
        let fs = MemFs::new();
        let l = run_load(&fs);
        assert_eq!(l.data, Data::default());
        assert_eq!(l.notice, None);
        assert_eq!(l.write_back_error, None);
        assert!(!l.save_blocked);
        assert!(fs.names().is_empty());
    }

    #[test]
    fn corrupt_file_renamed_and_restored_from_latest_good_backup() {
        let fs = MemFs::new();
        fs.put(paths().data(), b"{broken");
        fs.put(paths().backup_for(d(10, 5)), GOOD_A.as_bytes());
        fs.put(paths().backup_for(d(10, 6)), b"also broken");
        let l = run_load(&fs);
        let corrupt = paths().corrupt("20261007-090203");
        assert_eq!(l.data, sample());
        assert_eq!(
            l.notice,
            Some(LoadNotice::RestoredCorrupt {
                backup_date: d(10, 5),
                corrupt_path: corrupt.clone()
            })
        );
        assert_eq!(l.write_back_error, None);
        assert_eq!(fs.get(&corrupt).unwrap(), b"{broken");
        assert_eq!(
            parse_data(&fs.get(&paths().data()).unwrap()).unwrap(),
            sample()
        );
    }

    #[test]
    fn corrupt_name_never_overwrites_existing_corrupt_file() {
        let fs = MemFs::new();
        fs.put(paths().data(), b"new bad");
        fs.put(paths().corrupt("20261007-090203"), b"old bad 0");
        fs.put(paths().corrupt("20261007-090203-1"), b"old bad 1");
        let l = run_load(&fs);
        let expected = paths().corrupt("20261007-090203-2");
        assert_eq!(
            l.notice,
            Some(LoadNotice::CorruptNoBackup {
                corrupt_path: expected.clone()
            })
        );
        assert_eq!(fs.get(&expected).unwrap(), b"new bad");
        assert_eq!(
            fs.get(&paths().corrupt("20261007-090203")).unwrap(),
            b"old bad 0"
        );
    }

    #[test]
    fn missing_file_restored_from_backup() {
        let fs = MemFs::new();
        fs.put(paths().backup_for(d(10, 4)), GOOD_B.as_bytes());
        fs.put(paths().backup_for(d(10, 6)), GOOD_A.as_bytes());
        let l = run_load(&fs);
        assert_eq!(l.data, sample());
        assert_eq!(
            l.notice,
            Some(LoadNotice::RestoredMissing {
                backup_date: d(10, 6)
            })
        );
        assert_eq!(
            parse_data(&fs.get(&paths().data()).unwrap()).unwrap(),
            sample()
        );
    }

    #[test]
    fn corrupt_without_backup_keeps_corrupt_and_starts_empty() {
        let fs = MemFs::new();
        fs.put(paths().data(), b"{broken");
        let l = run_load(&fs);
        let corrupt = paths().corrupt("20261007-090203");
        assert_eq!(l.data, Data::default());
        assert_eq!(
            l.notice,
            Some(LoadNotice::CorruptNoBackup {
                corrupt_path: corrupt.clone()
            })
        );
        assert_eq!(fs.get(&corrupt).unwrap(), b"{broken");
        assert!(!fs.exists(&paths().data()));
        assert!(!l.save_blocked);
    }

    #[test]
    fn unreadable_data_file_is_untouched_and_blocks_saving() {
        let fs = MemFs::new();
        fs.put(paths().data(), GOOD_B.as_bytes());
        fs.put(paths().backup_for(d(10, 5)), GOOD_A.as_bytes());
        *fs.fail_reads_of.borrow_mut() = Some(paths().data());
        let l = run_load(&fs);
        assert_eq!(l.data, sample());
        assert!(l.save_blocked);
        assert!(matches!(
            l.notice,
            Some(LoadNotice::Unreadable {
                backup_date: Some(date),
                ..
            }) if date == d(10, 5)
        ));
        assert_eq!(fs.get(&paths().data()).unwrap(), GOOD_B.as_bytes());
        assert!(
            fs.names()
                .iter()
                .all(|p| !p.to_string_lossy().contains("corrupt"))
        );
    }

    #[test]
    fn unreadable_without_backup_is_empty_and_blocked() {
        let fs = MemFs::new();
        fs.put(paths().data(), GOOD_B.as_bytes());
        *fs.fail_reads_of.borrow_mut() = Some(paths().data());
        let l = run_load(&fs);
        assert_eq!(l.data, Data::default());
        assert!(l.save_blocked);
        assert!(matches!(
            l.notice,
            Some(LoadNotice::Unreadable {
                backup_date: None,
                ..
            })
        ));
    }

    #[test]
    fn rename_failure_leaves_corrupt_original_and_blocks_saving() {
        let fs = MemFs::new();
        fs.put(paths().data(), b"{broken");
        fs.put(paths().backup_for(d(10, 5)), GOOD_A.as_bytes());
        fs.fail_writes.set(true);
        let l = run_load(&fs);
        assert_eq!(l.data, sample());
        assert!(l.save_blocked);
        assert!(matches!(l.notice, Some(LoadNotice::Unreadable { .. })));
        assert_eq!(fs.get(&paths().data()).unwrap(), b"{broken");
    }

    #[test]
    fn write_back_failure_is_reported_but_data_returned() {
        let fs = MemFs::new();
        fs.put(paths().backup_for(d(10, 5)), GOOD_A.as_bytes());
        fs.fail_writes.set(true);
        let l = run_load(&fs);
        assert_eq!(l.data, sample());
        assert!(l.write_back_error.is_some());
        assert!(!l.save_blocked);
    }

    #[test]
    fn leftover_tmp_is_ignored() {
        let fs = MemFs::new();
        fs.put(paths().tmp(), GOOD_A.as_bytes());
        let l = run_load(&fs);
        assert_eq!(l.data, Data::default());
        assert_eq!(l.notice, None);
        assert_eq!(fs.get(&paths().tmp()).unwrap(), GOOD_A.as_bytes());
    }

    #[test]
    fn only_exactly_named_backups_count() {
        let fs = MemFs::new();
        let b = paths().backups();
        fs.put(b.join("data-2026-10-09.json.tmp"), GOOD_B.as_bytes());
        fs.put(b.join("data-2026-10-09.json.bak"), GOOD_B.as_bytes());
        fs.put(b.join("data-2026-1-9.json"), GOOD_B.as_bytes());
        fs.put(b.join("notes.json"), GOOD_B.as_bytes());
        fs.put(paths().backup_for(d(10, 1)), GOOD_A.as_bytes());
        let l = run_load(&fs);
        assert_eq!(l.data, sample());
        assert_eq!(
            l.notice,
            Some(LoadNotice::RestoredMissing {
                backup_date: d(10, 1)
            })
        );
    }

    #[test]
    fn daily_backup_once_per_day() {
        let fs = MemFs::new();
        assert!(!ensure_daily_backup(&fs, &paths(), d(10, 6)).unwrap());
        fs.put(paths().data(), GOOD_A.as_bytes());
        assert!(ensure_daily_backup(&fs, &paths(), d(10, 6)).unwrap());
        assert_eq!(
            fs.get(&paths().backup_for(d(10, 6))).unwrap(),
            GOOD_A.as_bytes()
        );
        fs.put(paths().data(), GOOD_B.as_bytes());
        assert!(!ensure_daily_backup(&fs, &paths(), d(10, 6)).unwrap());
        assert_eq!(
            fs.get(&paths().backup_for(d(10, 6))).unwrap(),
            GOOD_A.as_bytes()
        );
        assert!(ensure_daily_backup(&fs, &paths(), d(10, 7)).unwrap());
    }

    #[test]
    fn daily_backup_prunes_to_30() {
        let fs = MemFs::new();
        fs.put(paths().data(), GOOD_A.as_bytes());
        let other = paths().backups().join("keep-me.txt");
        fs.put(other.clone(), b"x");
        let start = d(8, 1);
        for i in 0..35 {
            let day = start + chrono::Days::new(i);
            assert!(ensure_daily_backup(&fs, &paths(), day).unwrap());
        }
        let backups = list_backups(&fs, &paths()).unwrap();
        assert_eq!(backups.len(), BACKUP_KEEP);
        assert_eq!(backups[0].0, start + chrono::Days::new(34));
        assert_eq!(backups[29].0, start + chrono::Days::new(5));
        assert!(fs.exists(&other));
        assert!(
            !fs.names()
                .iter()
                .any(|p| p.to_string_lossy().ends_with(".tmp"))
        );
    }

    #[test]
    fn daily_backup_failure_leaves_no_partial_file() {
        let fs = MemFs::new();
        fs.put(paths().data(), GOOD_A.as_bytes());
        fs.fail_writes.set(true);
        assert!(ensure_daily_backup(&fs, &paths(), d(10, 6)).is_err());
        assert!(!fs.exists(&paths().backup_for(d(10, 6))));
    }

    #[test]
    fn real_fs_atomic_replace() {
        let dir = tempfile::tempdir().unwrap();
        let p = Paths {
            dir: dir.path().join("partimer"),
        };
        save_atomic(&RealFs, &p, &sample()).unwrap();
        let second = parse_data(GOOD_B.as_bytes()).unwrap();
        save_atomic(&RealFs, &p, &second).unwrap();
        let l = load(&RealFs, &p, tz("Asia/Taipei").as_ref(), now());
        assert_eq!(l.data, second);
        assert!(!p.tmp().exists());
        assert!(RealFs.list(&p.backups()).unwrap().is_empty());
        assert!(RealFs.list(&p.dir.join("nope")).unwrap().is_empty());
    }

    #[test]
    fn default_dir_ends_with_partimer() {
        if let Some(p) = default_dir() {
            assert!(p.ends_with("partimer"));
        }
    }

    #[test]
    fn corrupt_data_produces_no_backup() {
        let fs = MemFs::new();
        fs.put(paths().data(), b"{broken");
        assert!(!ensure_daily_backup(&fs, &paths(), d(10, 6)).unwrap());
        assert!(!fs.exists(&paths().backup_for(d(10, 6))));
        assert_eq!(fs.names(), vec![paths().data()]);
    }

    #[test]
    fn backup_rename_failure_leaves_no_final_file_and_retry_works() {
        let fs = MemFs::new();
        fs.put(paths().data(), GOOD_A.as_bytes());
        fs.fail_renames.set(true);
        assert!(ensure_daily_backup(&fs, &paths(), d(10, 6)).is_err());
        assert!(!fs.exists(&paths().backup_for(d(10, 6))));
        assert!(
            !fs.names()
                .iter()
                .any(|p| p.to_string_lossy().ends_with(".tmp"))
        );
        fs.fail_renames.set(false);
        assert!(ensure_daily_backup(&fs, &paths(), d(10, 6)).unwrap());
    }

    #[test]
    fn save_rename_failure_keeps_old_file() {
        let fs = MemFs::new();
        fs.put(paths().data(), GOOD_B.as_bytes());
        fs.fail_renames.set(true);
        assert!(save_atomic(&fs, &paths(), &sample()).is_err());
        assert_eq!(fs.get(&paths().data()).unwrap(), GOOD_B.as_bytes());
        assert!(
            !fs.exists(&paths().tmp()),
            "tmp removed after a failed rename"
        );
    }

    #[test]
    fn backup_folder_list_error_is_reported() {
        let fs = MemFs::new();
        fs.put(paths().backup_for(d(10, 5)), GOOD_A.as_bytes());
        fs.fail_lists.set(true);
        let l = run_load(&fs);
        assert_eq!(l.data, Data::default());
        let io = io::Error::from(io::ErrorKind::PermissionDenied).to_string();
        assert_eq!(l.backup_error, Some(io));
        // Reading data.json fine never looks at the backups.
        let fs = MemFs::new();
        fs.put(paths().data(), GOOD_A.as_bytes());
        fs.fail_lists.set(true);
        assert_eq!(run_load(&fs).backup_error, None);
    }

    #[test]
    fn future_dated_backup_does_not_win() {
        // now = 2026-10-07 local; a backup "from December" comes from a clock jump.
        let fs = MemFs::new();
        fs.put(paths().backup_for(d(10, 5)), GOOD_A.as_bytes());
        fs.put(paths().backup_for(d(12, 1)), GOOD_B.as_bytes());
        let l = run_load(&fs);
        assert_eq!(l.data, sample());
        assert_eq!(
            l.notice,
            Some(LoadNotice::RestoredMissing {
                backup_date: d(10, 5)
            })
        );
    }

    #[test]
    fn only_future_dated_backup_is_still_used() {
        let fs = MemFs::new();
        fs.put(paths().backup_for(d(12, 1)), GOOD_A.as_bytes());
        let l = run_load(&fs);
        assert_eq!(l.data, sample());
        assert_eq!(
            l.notice,
            Some(LoadNotice::RestoredMissing {
                backup_date: d(12, 1)
            })
        );
    }

    #[test]
    fn corrupt_file_stays_when_no_free_name_can_be_confirmed() {
        let fs = MemFs::new();
        fs.put(paths().data(), b"{broken");
        fs.put(paths().backup_for(d(10, 5)), GOOD_A.as_bytes());
        fs.exists_unknown.set(true);
        let l = run_load(&fs);
        assert_eq!(l.data, sample());
        assert!(l.save_blocked);
        assert!(matches!(
            l.notice,
            Some(LoadNotice::Unreadable {
                reason: UnreadableReason::CannotKeepCorrupt,
                backup_date: Some(_),
                ..
            })
        ));
        assert_eq!(fs.get(&paths().data()).unwrap(), b"{broken");
        assert!(
            fs.names()
                .iter()
                .all(|p| !p.to_string_lossy().contains("corrupt"))
        );
    }

    #[test]
    fn unreadable_error_is_the_raw_io_error() {
        let fs = MemFs::new();
        fs.put(paths().data(), GOOD_B.as_bytes());
        *fs.fail_reads_of.borrow_mut() = Some(paths().data());
        let io = io::Error::from(io::ErrorKind::PermissionDenied).to_string();
        assert_eq!(
            run_load(&fs).notice,
            Some(LoadNotice::Unreadable {
                reason: UnreadableReason::ReadFailed,
                error: io,
                backup_date: None,
            })
        );
        let fs = MemFs::new();
        fs.put(paths().data(), b"{broken");
        fs.fail_renames.set(true);
        let storage_full = io::Error::from(io::ErrorKind::StorageFull).to_string();
        assert_eq!(
            run_load(&fs).notice,
            Some(LoadNotice::Unreadable {
                reason: UnreadableReason::CannotKeepCorrupt,
                error: storage_full,
                backup_date: None,
            })
        );
    }

    #[test]
    fn exists_errors_count_as_existing() {
        assert!(exists_or_unknown(Ok(true)));
        assert!(!exists_or_unknown(Ok(false)));
        assert!(exists_or_unknown(Err(
            io::ErrorKind::PermissionDenied.into()
        )));
    }

    #[test]
    fn corrupt_restore_write_back_failure_is_reported() {
        let fs = MemFs::new();
        fs.put(paths().data(), b"{broken");
        fs.put(paths().backup_for(d(10, 5)), GOOD_A.as_bytes());
        fs.fail_write_syncs.set(true);
        let l = run_load(&fs);
        let corrupt = paths().corrupt("20261007-090203");
        assert_eq!(l.data, sample());
        assert_eq!(
            l.notice,
            Some(LoadNotice::RestoredCorrupt {
                backup_date: d(10, 5),
                corrupt_path: corrupt.clone()
            })
        );
        let storage_full = io::Error::from(io::ErrorKind::StorageFull).to_string();
        assert_eq!(l.write_back_error, Some(storage_full));
        assert!(!l.save_blocked);
        assert_eq!(fs.get(&corrupt).unwrap(), b"{broken");
        assert!(!fs.exists(&paths().data()));
        assert!(!fs.exists(&paths().tmp()));
    }
}
