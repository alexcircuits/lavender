//! Size-limited disk cache and sweep cleaner for traffic flow tiles.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::doc_markdown,
    clippy::must_use_candidate,
    clippy::collapsible_if
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Information about a cached disk file.
#[derive(Debug)]
pub struct DiskFileInfo {
    pub path: PathBuf,
    pub size: u64,
    pub mtime: SystemTime,
}

/// Recursively collect all regular files in directory with metadata.
pub fn collect_cache_files(dir: &Path) -> Vec<DiskFileInfo> {
    let mut files = Vec::new();
    if !dir.is_dir() {
        return files;
    }

    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(read_dir) = fs::read_dir(&current) else {
            continue;
        };
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.is_file() {
                // Don't sweep the budget.json file!
                if path.file_name().and_then(|n| n.to_str()) == Some("budget.json") {
                    continue;
                }
                if let Ok(meta) = entry.metadata() {
                    let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                    files.push(DiskFileInfo { path, size: meta.len(), mtime });
                }
            }
        }
    }

    files
}

/// Sweeps the disk cache:
/// 1. Removes expired files older than `ttl`.
/// 2. Evicts oldest files until remaining total byte size is under `max_bytes`.
///
/// Returns `(removed_file_count, remaining_bytes)`.
pub fn sweep_disk_cache(dir: &Path, max_bytes: u64, ttl: Duration) -> (usize, u64) {
    let mut files = collect_cache_files(dir);
    // Sort oldest first
    files.sort_by_key(|f| f.mtime);

    let mut total_bytes: u64 = files.iter().map(|f| f.size).sum();
    let mut removed = 0;
    let now = SystemTime::now();

    for file in files {
        let age = now.duration_since(file.mtime).unwrap_or(Duration::ZERO);
        let expired = age > ttl;
        let over_limit = total_bytes > max_bytes;

        if expired || over_limit {
            if fs::remove_file(&file.path).is_ok() {
                total_bytes = total_bytes.saturating_sub(file.size);
                removed += 1;
            }
        }
    }

    (removed, total_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disk_cache_sweep() {
        let temp_dir =
            std::env::temp_dir().join(format!("lavender-cache-sweep-{}", std::process::id()));
        fs::create_dir_all(&temp_dir).expect("create temp dir");

        let file1 = temp_dir.join("flow-12-1-1.pbf");
        let file2 = temp_dir.join("flow-12-1-2.pbf");
        let file3 = temp_dir.join("flow-12-1-3.pbf");
        let budget_file = temp_dir.join("budget.json");

        // Write 3 files of 100 bytes each
        fs::write(&file1, vec![0u8; 100]).expect("write file1");
        fs::write(&file2, vec![1u8; 100]).expect("write file2");
        fs::write(&file3, vec![2u8; 100]).expect("write file3");
        fs::write(&budget_file, b"{\"date\":\"2026-10-22\",\"count\":5}").expect("write budget");

        // Sweep with max_bytes = 150 bytes, long TTL: should remove oldest files until <= 150 bytes
        // Out of 300 bytes in 3 files, at least 2 files must be removed so that remaining is <= 150 bytes (1 file = 100 bytes).
        let (removed, remaining) = sweep_disk_cache(&temp_dir, 150, Duration::from_secs(3600));

        assert!(removed >= 2, "must remove at least 2 files to drop below 150 bytes");
        assert!(remaining <= 150, "remaining bytes must be <= 150");
        assert!(budget_file.exists(), "budget.json must never be swept");

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }
}
