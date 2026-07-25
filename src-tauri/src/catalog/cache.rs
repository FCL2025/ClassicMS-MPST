//! Catalog 磁碟快取與 bundle manifest 驗證。
//!
//! wasm（瀏覽器）無傳統檔案系統：快取鎖、備份救援、隔離等桌面包裝
//! 只在非 wasm 下編譯；wasm 的快取讀寫會在 Step 2 改走 JS import 的
//! OPFS 同步 I/O（單一 JSON、單寫者，不需要鎖與原子 rename 那套）。

#[cfg(not(target_arch = "wasm32"))]
use std::io::Write;
use std::path::{Path, PathBuf};

use super::Result;
#[cfg(not(target_arch = "wasm32"))]
use super::{BundleMeta, CatalogError};

#[cfg(not(target_arch = "wasm32"))]
pub(super) struct CacheFileLock {
    path: PathBuf,
}

#[cfg(not(target_arch = "wasm32"))]
impl CacheFileLock {
    pub(super) fn acquire(cache_path: &Path) -> Result<Self> {
        let parent = cache_path.parent().unwrap_or_else(|| Path::new("."));
        let file_name = cache_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("catalog.json");
        let path = parent.join(format!(".{file_name}.lock"));
        for _ in 0..100 {
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(mut file) => {
                    writeln!(file, "{}", std::process::id())?;
                    file.sync_all()?;
                    return Ok(Self { path });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    let stale = std::fs::metadata(&path)
                        .and_then(|metadata| metadata.modified())
                        .ok()
                        .and_then(|modified| modified.elapsed().ok())
                        .is_some_and(|age| age > std::time::Duration::from_secs(300));
                    if stale {
                        let _ = std::fs::remove_file(&path);
                    } else {
                        std::thread::sleep(std::time::Duration::from_millis(20));
                    }
                }
                Err(error) => return Err(error.into()),
            }
        }
        Err(CatalogError::CacheInvalid(format!(
            "等待快取鎖逾時: {}",
            path.display()
        )))
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for CacheFileLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub(super) fn bundle_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let path = entry?.path();
        if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("bundle"))
        {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn bundle_manifest(root: &Path) -> Result<Vec<BundleMeta>> {
    bundle_files(root)?
        .into_iter()
        .map(|path| {
            let metadata = std::fs::metadata(&path)?;
            let name = path
                .file_stem()
                .ok_or_else(|| {
                    CatalogError::CacheInvalid(format!("bundle 無檔名: {}", path.display()))
                })?
                .to_string_lossy()
                .into_owned();
            Ok(BundleMeta {
                name,
                size: metadata.len(),
                mtime_nanos: modified_nanos(&metadata),
            })
        })
        .collect()
}

pub(super) fn modified_nanos(metadata: &std::fs::Metadata) -> u128 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_nanos())
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn cache_backup_path(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("catalog.json");
    parent.join(format!(".{file_name}.bak"))
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn read_cache_with_backup(path: &Path) -> std::io::Result<String> {
    match std::fs::read_to_string(path) {
        Ok(json) => Ok(json),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::read_to_string(cache_backup_path(path))
        }
        Err(error) => Err(error),
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn quarantine_cache(path: &Path) {
    if !path.exists() {
        return;
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("catalog.json");
    let corrupt = parent.join(format!(
        "{file_name}.corrupt.{}.{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_secs())
    ));
    let _ = std::fs::rename(path, corrupt);
}
