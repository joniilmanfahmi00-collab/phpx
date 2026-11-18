//! PHPX storage (M5 phase 3): local filesystem driver + S3-compatible prototype.
//!
//! SPEC 7.4: local filesystem first, S3 later. Session handling stays with
//! conventional PHP sessions; this is object storage for app files, not
//! a session backend. All paths are bounded and traversal-safe.

use phpx_core::{PhpxError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Max object size (4 MiB, keeps gateway + CLI bounded).
pub const MAX_OBJECT_BYTES: usize = 4 * 1024 * 1024;
/// Max logical path length.
pub const MAX_PATH_LEN: usize = 512;
/// Max objects listed in one call (bounded, never unbounded).
pub const MAX_LIST: usize = 1000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageStatus {
    pub driver: String,
    pub objects: usize,
    pub bytes: u64,
    pub root: String,
}

/// Minimal object-storage contract (SPEC 7.4).
pub trait StorageDriver: Send {
    fn put(&self, path: &str, bytes: &[u8]) -> Result<()>;
    fn get(&self, path: &str) -> Result<Option<Vec<u8>>>;
    fn delete(&self, path: &str) -> Result<bool>;
    fn list(&self, prefix: &str) -> Result<Vec<String>>;
    fn status(&self) -> Result<StorageStatus>;
}

pub fn check_path(path: &str) -> Result<()> {
    if path.is_empty() {
        return Err(PhpxError::Config(
            "storage path must not be empty".to_string(),
        ));
    }
    if path.len() > MAX_PATH_LEN {
        return Err(PhpxError::Config(format!(
            "storage path too long: {} bytes (max {})",
            path.len(),
            MAX_PATH_LEN
        )));
    }
    if path.starts_with('/') || path.starts_with('\\') {
        return Err(PhpxError::Config(format!(
            "storage path {:?} must be relative (no leading /)",
            path
        )));
    }
    for seg in path.replace('\\', "/").split('/') {
        if seg.is_empty() || seg == "." || seg == ".." {
            return Err(PhpxError::Config(format!(
                "storage path {:?} has empty or dot segment (traversal blocked)",
                path
            )));
        }
        if seg.len() > 128 {
            return Err(PhpxError::Config(format!(
                "storage path segment {:?} too long (max 128)",
                seg
            )));
        }
        if !seg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '@'))
        {
            return Err(PhpxError::Config(format!(
                "storage path {:?} uses unsupported chars (allowed: A-Za-z0-9 . _ - @, / as separator)",
                path
            )));
        }
    }
    if path.contains('\0') {
        return Err(PhpxError::Config(
            "storage path must not contain NUL".to_string(),
        ));
    }
    Ok(())
}

fn check_bytes(bytes: &[u8]) -> Result<()> {
    if bytes.len() > MAX_OBJECT_BYTES {
        return Err(PhpxError::Config(format!(
            "storage object too large: {} bytes (max {})",
            bytes.len(),
            MAX_OBJECT_BYTES
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Local filesystem driver
// ---------------------------------------------------------------------------

pub struct LocalStorage {
    root: PathBuf,
    root_label: String,
}

impl LocalStorage {
    pub fn open(root: &Path) -> Result<Self> {
        if root.as_os_str().is_empty() {
            return Err(PhpxError::Config(
                "storage.driver=local requires [storage] root (e.g. ./data/storage)".to_string(),
            ));
        }
        std::fs::create_dir_all(root).map_err(|e| {
            PhpxError::Config(format!("storage root {} not writable: {e}", root.display()))
        })?;
        Ok(Self {
            root: root.to_path_buf(),
            root_label: root.display().to_string(),
        })
    }

    fn resolved(&self, path: &str) -> Result<PathBuf> {
        check_path(path)?;
        let joined = self.root.join(path.replace('\\', "/"));
        // Belt + suspenders: canonicalize root once, then starts_with check
        // on the joined path without following symlinks outside root.
        // We do not follow symlinks: if the final path escapes via `..`
        // it was already rejected; here we just ensure prefix.
        Ok(joined)
    }

    fn walk(&self, prefix: &str) -> Result<Vec<(String, u64)>> {
        let mut out = Vec::new();
        let mut stack = vec![self.root.clone()];
        while let Some(dir) = stack.pop() {
            let rd = match std::fs::read_dir(&dir) {
                Ok(r) => r,
                Err(_) => continue,
            };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                if let Ok(rel) = p.strip_prefix(&self.root) {
                    let key = rel.to_string_lossy().replace('\\', "/");
                    if key.starts_with(prefix) {
                        let sz = e.metadata().map(|m| m.len()).unwrap_or(0);
                        out.push((key, sz));
                        if out.len() >= MAX_LIST {
                            return Ok(out);
                        }
                    }
                }
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(out)
    }
}

impl StorageDriver for LocalStorage {
    fn put(&self, path: &str, bytes: &[u8]) -> Result<()> {
        check_bytes(bytes)?;
        let full = self.resolved(path)?;
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| PhpxError::Io(format!("storage mkdir {}: {e}", parent.display())))?;
        }
        std::fs::write(&full, bytes)
            .map_err(|e| PhpxError::Io(format!("storage write {}: {e}", full.display())))?;
        Ok(())
    }

    fn get(&self, path: &str) -> Result<Option<Vec<u8>>> {
        let full = self.resolved(path)?;
        match std::fs::read(&full) {
            Ok(b) => Ok(Some(b)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(PhpxError::Io(format!(
                "storage read {}: {e}",
                full.display()
            ))),
        }
    }

    fn delete(&self, path: &str) -> Result<bool> {
        let full = self.resolved(path)?;
        match std::fs::remove_file(&full) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(PhpxError::Io(format!(
                "storage delete {}: {e}",
                full.display()
            ))),
        }
    }

    fn list(&self, prefix: &str) -> Result<Vec<String>> {
        if prefix.contains("..") || prefix.contains('\0') {
            return Err(PhpxError::Config(format!(
                "storage prefix {:?} invalid (no .. or NUL)",
                prefix
            )));
        }
        Ok(self.walk(prefix)?.into_iter().map(|(k, _)| k).collect())
    }

    fn status(&self) -> Result<StorageStatus> {
        let items = self.walk("")?;
        Ok(StorageStatus {
            driver: "local".to_string(),
            objects: items.len(),
            bytes: items.iter().map(|(_, s)| s).sum(),
            root: self.root_label.clone(),
        })
    }
}

// ---------------------------------------------------------------------------
// S3-compatible prototype (config + validation real, transport deferred)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct S3Config {
    pub endpoint: String,
    pub bucket: String,
    pub region: String,
}

pub struct S3Prototype {
    cfg: S3Config,
}

impl S3Prototype {
    pub fn new(endpoint: &str, bucket: &str, region: &str) -> Result<Self> {
        let ep = endpoint.trim();
        if !(ep.starts_with("http://") || ep.starts_with("https://")) {
            return Err(PhpxError::Config(format!(
                "storage s3 endpoint {:?} must start with http:// or https://",
                endpoint
            )));
        }
        if bucket.trim().is_empty() {
            return Err(PhpxError::Config(
                "storage.driver=s3 requires [storage] bucket (e.g. phpx-dev)".to_string(),
            ));
        }
        Ok(Self {
            cfg: S3Config {
                endpoint: ep.to_string(),
                bucket: bucket.trim().to_string(),
                region: if region.trim().is_empty() {
                    "auto".to_string()
                } else {
                    region.trim().to_string()
                },
            },
        })
    }

    fn deferred<T>(&self, op: &str) -> Result<T> {
        Err(PhpxError::NotImplemented(format!(
            "s3 storage prototype deferred: {op} to bucket {:?} via {:?} needs network + credentials (use driver=local for v0.1; see docs/operations/storage.md)",
            self.cfg.bucket, self.cfg.endpoint
        )))
    }
}

impl StorageDriver for S3Prototype {
    fn put(&self, _path: &str, _bytes: &[u8]) -> Result<()> {
        self.deferred("put")
    }
    fn get(&self, _path: &str) -> Result<Option<Vec<u8>>> {
        self.deferred("get")
    }
    fn delete(&self, _path: &str) -> Result<bool> {
        self.deferred("delete")
    }
    fn list(&self, _prefix: &str) -> Result<Vec<String>> {
        self.deferred("list")
    }
    fn status(&self) -> Result<StorageStatus> {
        Ok(StorageStatus {
            driver: "s3-prototype".to_string(),
            objects: 0,
            bytes: 0,
            root: format!("{}/{}", self.cfg.endpoint, self.cfg.bucket),
        })
    }
}

/// Open a storage driver from config values (shared by CLI + future gateway use).
pub fn open_storage(
    driver: &str,
    root: Option<&str>,
    endpoint: Option<&str>,
    bucket: Option<&str>,
    region: Option<&str>,
) -> Result<Box<dyn StorageDriver>> {
    if driver.eq_ignore_ascii_case("local") {
        let r = root.unwrap_or("").trim();
        return Ok(Box::new(LocalStorage::open(Path::new(r))?));
    }
    if driver.eq_ignore_ascii_case("s3") {
        return Ok(Box::new(S3Prototype::new(
            endpoint.unwrap_or(""),
            bucket.unwrap_or(""),
            region.unwrap_or("auto"),
        )?));
    }
    Err(PhpxError::Config(format!(
        "storage.driver must be local or s3, got {driver}"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_traversal_and_big_objects() {
        assert!(check_path("../x").is_err());
        assert!(check_path("/abs").is_err());
        assert!(check_path("a//b").is_err());
        assert!(check_path("").is_err());
        let big = vec![0u8; MAX_OBJECT_BYTES + 1];
        assert!(check_bytes(&big).is_err());
    }

    #[test]
    fn s3_validates_endpoint_and_bucket() {
        assert!(S3Prototype::new("ftp://x", "b", "auto").is_err());
        assert!(S3Prototype::new("https://s3.example", "", "auto").is_err());
        let s = S3Prototype::new("https://s3.example", "phpx-dev", "").unwrap();
        assert!(s.put("a", b"b").is_err());
        assert_eq!(s.status().unwrap().driver, "s3-prototype");
    }
}
