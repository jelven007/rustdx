//! Bounded copies, durable directory entries and process-scoped single-host exclusion.
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

use sha2::{Digest, Sha256};

use crate::{Result, config, invalid, model::Evidence};

pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn read_limited(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(invalid(format!(
            "file exceeds {limit} bytes: {}",
            path.display()
        )));
    }
    Ok(bytes)
}

pub fn safe_join(root: &Path, relative: &str) -> Result<PathBuf> {
    let path = Path::new(relative);
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(invalid(
            "artifact path must be a nonempty relative normal path",
        ));
    }
    let mut current = root.to_owned();
    for component in path.components() {
        current.push(component);
        if fs::symlink_metadata(&current)?.file_type().is_symlink() {
            return Err(invalid("artifact symlinks are forbidden"));
        }
    }
    Ok(current)
}

pub fn private_directory(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

pub fn sync_directory(path: &Path) -> Result<()> {
    File::open(path)?.sync_all()?;
    Ok(())
}

pub(crate) fn freeze_tree(path: &Path) -> Result<()> {
    for entry in fs::read_dir(path)? {
        let path = entry?.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            return Err(invalid("cannot freeze symlinks"));
        }
        if metadata.is_dir() {
            freeze_tree(&path)?;
        } else if metadata.is_file() {
            File::open(&path)?.sync_all()?;
            let mut permissions = metadata.permissions();
            permissions.set_readonly(true);
            fs::set_permissions(&path, permissions)?;
        } else {
            return Err(invalid("cannot freeze special files"));
        }
    }
    sync_directory(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o500))?;
    }
    Ok(())
}

pub fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    sync_directory(path.parent().ok_or_else(|| invalid("missing parent"))?)
}

pub fn hash_file(path: &Path, limit: u64) -> Result<(String, u64)> {
    let mut reader = File::open(path)?;
    if !reader.metadata()?.is_file() {
        return Err(invalid("evidence must be a regular file"));
    }
    let mut hash = Sha256::new();
    let mut bytes = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        rustdx_complete::operation::check().map_err(crate::Error::from)?;
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        bytes = bytes
            .checked_add(n as u64)
            .ok_or_else(|| invalid("file size overflow"))?;
        if bytes > limit {
            return Err(invalid("evidence byte budget exceeded"));
        }
        hash.update(&buffer[..n]);
    }
    Ok((format!("{:x}", hash.finalize()), bytes))
}

pub fn evidence(root: &Path, path: &Path, limit: u64) -> Result<Evidence> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| invalid("evidence outside root"))?
        .to_str()
        .ok_or_else(|| invalid("non-UTF8 evidence path"))?;
    let (sha256, bytes) = hash_file(&safe_join(root, relative)?, limit)?;
    Ok(Evidence {
        path: relative.into(),
        sha256,
        bytes,
    })
}

pub fn verify(root: &Path, item: &Evidence) -> Result<()> {
    config::digest(&item.sha256)?;
    let (hash, bytes) = hash_file(&safe_join(root, &item.path)?, item.bytes)?;
    if hash != item.sha256 || bytes != item.bytes {
        return Err(invalid(format!("evidence changed: {}", item.path)));
    }
    Ok(())
}

pub fn copy_new(source: &Path, destination: &Path, limit: u64) -> Result<(String, u64)> {
    if fs::symlink_metadata(source)?.file_type().is_symlink() {
        return Err(invalid("source symlinks are forbidden"));
    }
    let mut input = File::open(source)?;
    if !input.metadata()?.is_file() {
        return Err(invalid("source must be a regular file"));
    }
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    let mut hash = Sha256::new();
    let mut bytes = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        rustdx_complete::operation::check().map_err(crate::Error::from)?;
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        bytes = bytes
            .checked_add(count as u64)
            .ok_or_else(|| invalid("size overflow"))?;
        if bytes > limit {
            return Err(invalid("source byte budget exceeded"));
        }
        output.write_all(&buffer[..count])?;
        hash.update(&buffer[..count]);
    }
    output.sync_all()?;
    sync_directory(
        destination
            .parent()
            .ok_or_else(|| invalid("missing parent"))?,
    )?;
    Ok((format!("{:x}", hash.finalize()), bytes))
}

pub struct RootLock {
    _file: File,
    root: PathBuf,
}

impl RootLock {
    pub fn acquire(root: &Path) -> Result<Self> {
        private_directory(root)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join("controller.lock"))?;
        file.try_lock().map_err(|e| {
            invalid(format!(
                "controller already running or lock unavailable: {e}"
            ))
        })?;
        Ok(Self {
            _file: file,
            root: root.canonicalize()?,
        })
    }

    pub(crate) fn covers(&self, path: &Path) -> Result<()> {
        if path
            .parent()
            .ok_or_else(|| invalid("ledger has no parent"))?
            .canonicalize()?
            != self.root
        {
            return Err(invalid("controller lock belongs to another root"));
        }
        Ok(())
    }
}

/// Resolve an existing source and the parent of a new destination before any write.
/// Parent aliases and `..` cannot place an ostensibly external copy inside source.
pub(crate) fn isolated_destination(source: &Path, destination: &Path) -> Result<PathBuf> {
    let source = source.canonicalize()?;
    let name = destination
        .file_name()
        .ok_or_else(|| invalid("destination requires a filename"))?;
    if destination
        .components()
        .any(|c| matches!(c, Component::ParentDir))
    {
        return Err(invalid("destination parent traversal is forbidden"));
    }
    let parent = destination
        .parent()
        .ok_or_else(|| invalid("destination has no parent"))?
        .canonicalize()?;
    let target = parent.join(name);
    if target.starts_with(&source) || fs::symlink_metadata(&target).is_ok() {
        return Err(invalid(
            "destination must be new and outside the source root",
        ));
    }
    Ok(target)
}

/// Counts regular files without following symlinks and caps filesystem traversal.
pub fn tree_bytes(root: &Path) -> Result<u64> {
    Ok(tree_usage(root)?.bytes)
}

pub(crate) const MAX_TREE_ENTRIES: u64 = 200_000;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct TreeUsage {
    pub bytes: u64,
    pub entries: u64,
}

/// Advisory accounting after inputs stop changing. Final reconciliation still scans everything.
pub(crate) struct WorkspaceMeter {
    root: PathBuf,
    fixed: TreeUsage,
    dynamic: Vec<String>,
}

fn path_usage(path: &Path) -> Result<TreeUsage> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(TreeUsage::default()),
        Err(e) => return Err(e.into()),
    };
    let mut usage = if metadata.is_dir() {
        tree_usage(path)?
    } else if metadata.is_file() {
        TreeUsage {
            bytes: metadata.len(),
            entries: 0,
        }
    } else {
        return Err(invalid("workspace contains symlink or special file"));
    };
    usage.entries += 1;
    Ok(usage)
}

fn add_usage(total: &mut TreeUsage, usage: TreeUsage) -> Result<()> {
    total.bytes = total
        .bytes
        .checked_add(usage.bytes)
        .ok_or_else(|| invalid("size overflow"))?;
    total.entries = total
        .entries
        .checked_add(usage.entries)
        .ok_or_else(|| invalid("entry overflow"))?;
    if total.entries > MAX_TREE_ENTRIES {
        return Err(invalid("workspace entry budget exceeded"));
    }
    Ok(())
}

impl WorkspaceMeter {
    pub(crate) fn new(root: &Path, dynamic: &[&str]) -> Result<Self> {
        let mut fixed = TreeUsage::default();
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            if !dynamic.iter().any(|name| entry.file_name() == *name) {
                add_usage(&mut fixed, path_usage(&entry.path())?)?;
            }
        }
        Ok(Self {
            root: root.into(),
            fixed,
            dynamic: dynamic.iter().map(|name| (*name).into()).collect(),
        })
    }

    pub(crate) fn sample(&self, max_bytes: u64) -> Result<TreeUsage> {
        rustdx_complete::operation::check().map_err(crate::Error::from)?;
        let mut usage = self.fixed;
        for name in &self.dynamic {
            add_usage(&mut usage, path_usage(&self.root.join(name))?)?;
        }
        if usage.bytes > max_bytes {
            return Err(invalid("normalization exceeds workspace budget"));
        }
        Ok(usage)
    }

    pub(crate) fn reconcile(&self, max_bytes: u64) -> Result<()> {
        let estimated = self.sample(max_bytes)?;
        let actual = tree_usage(&self.root)?;
        if actual.bytes > max_bytes
            || actual.bytes != estimated.bytes
            || actual.entries != estimated.entries
        {
            return Err(invalid(
                "stationary workspace differs from normalization accounting",
            ));
        }
        Ok(())
    }
}

pub(crate) fn tree_usage(root: &Path) -> Result<TreeUsage> {
    let mut stack = vec![root.to_owned()];
    let mut entries = 0_u64;
    let mut bytes = 0_u64;
    while let Some(path) = stack.pop() {
        rustdx_complete::operation::check().map_err(crate::Error::from)?;
        // Workers atomically rename/remove private staging files while this
        // approximate monitor runs. Only disappeared entries are benign;
        // quotas provide the hard limit and sealing recounts a stationary tree.
        let directory = match fs::read_dir(&path) {
            Ok(directory) => directory,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && path != root => continue,
            Err(e) => return Err(e.into()),
        };
        for entry in directory {
            let entry = entry?;
            entries += 1;
            if entries.is_multiple_of(4096) {
                rustdx_complete::operation::check().map_err(crate::Error::from)?;
            }
            if entries > MAX_TREE_ENTRIES {
                return Err(invalid("workspace entry budget exceeded"));
            }
            let meta = match fs::symlink_metadata(entry.path()) {
                Ok(meta) => meta,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.into()),
            };
            if meta.file_type().is_symlink() {
                return Err(invalid("workspace contains symlink"));
            } else if meta.is_dir() {
                stack.push(entry.path());
            } else if meta.is_file() {
                bytes = bytes
                    .checked_add(meta.len())
                    .ok_or_else(|| invalid("size overflow"))?;
            } else {
                return Err(invalid("workspace contains special file"));
            }
        }
    }
    Ok(TreeUsage { bytes, entries })
}

#[cfg(unix)]
pub fn free_bytes(root: &Path) -> Result<u64> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let path = CString::new(root.as_os_str().as_bytes()).map_err(|_| invalid("NUL in path"))?;
    let mut info = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: path is NUL-terminated and info points to valid writable statvfs storage.
    if unsafe { libc::statvfs(path.as_ptr(), info.as_mut_ptr()) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    // SAFETY: successful statvfs initialized the whole structure.
    let info = unsafe { info.assume_init() };
    // libc uses different field widths on supported Unix targets.
    #[allow(clippy::unnecessary_cast)]
    let bytes = (info.f_bavail as u64).saturating_mul(info.f_frsize as u64);
    Ok(bytes)
}

#[cfg(not(unix))]
pub fn free_bytes(_root: &Path) -> Result<u64> {
    Err(invalid("runner capacity checks require Unix"))
}

#[cfg(test)]
mod meter_tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn dynamic_meter_counts_sqlite_journal_and_disappearing_files_and_reconciles() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        private_directory(&root.join("raw")).unwrap();
        write_new(&root.join("raw/input"), b"fixed").unwrap();
        let meter = WorkspaceMeter::new(
            root,
            &["normalization.sqlite", "normalization.sqlite-journal"],
        )
        .unwrap();
        let db = rusqlite::Connection::open(root.join("normalization.sqlite")).unwrap();
        db.execute_batch(
            "PRAGMA journal_mode=DELETE; CREATE TABLE data(v BLOB); BEGIN;
            INSERT INTO data VALUES(zeroblob(1048576));",
        )
        .unwrap();
        assert!(root.join("normalization.sqlite-journal").exists());
        assert_eq!(
            meter.sample(u64::MAX).unwrap().bytes,
            tree_bytes(root).unwrap()
        );
        assert!(meter.sample(1024).is_err());
        db.execute_batch("COMMIT;").unwrap();
        assert!(!root.join("normalization.sqlite-journal").exists());
        meter.reconcile(u64::MAX).unwrap();
        drop(db);
        fs::remove_file(root.join("normalization.sqlite")).unwrap();
        meter.reconcile(5).unwrap();
        // An unclassified addition cannot evade the stationary final scan.
        write_new(&root.join("unexpected"), b"x").unwrap();
        assert!(meter.reconcile(u64::MAX).is_err());
    }

    #[test]
    fn dynamic_meter_avoids_rescanning_fixed_file_history() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        private_directory(&root.join("raw")).unwrap();
        for n in 0..2000 {
            fs::write(root.join("raw").join(n.to_string()), [0; 64]).unwrap();
        }
        fs::write(root.join("normalization.sqlite"), [0; 4096]).unwrap();
        let meter = WorkspaceMeter::new(root, &["normalization.sqlite"]).unwrap();
        let start = Instant::now();
        for _ in 0..100 {
            assert_eq!(tree_bytes(root).unwrap(), 132096);
        }
        let full = start.elapsed();
        let start = Instant::now();
        for _ in 0..100 {
            assert_eq!(meter.sample(u64::MAX).unwrap().bytes, 132096);
        }
        let sampled = start.elapsed();
        meter.reconcile(132096).unwrap();
        println!(
            "fixed_files=2000 samples=100 full_scan_micros={} dynamic_sample_micros={}",
            full.as_micros(),
            sampled.as_micros()
        );
    }
}
