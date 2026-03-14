use chrono::{DateTime, Utc};
use directories_next as directories;
use hex;
use sha2::{Digest, Sha256};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub fn get_cache_dir() -> io::Result<PathBuf> {
    let mut cache_path;
    if let Some(base_dirs) = directories::BaseDirs::new() {
        cache_path = base_dirs.cache_dir().to_path_buf();
        cache_path.push("oculante_slint"); // Application specific directory
    } else {
        cache_path = PathBuf::from(".cache");
    }
    cache_path.push("thumbnails"); // Subdirectory for thumbnails
    fs::create_dir_all(&cache_path)?; // Propagate error if it happened
    Ok(cache_path)
}

pub fn get_thumbnail_path(original_path: &Path, cache_dir: &Path) -> Option<PathBuf> {
    if let Ok(metadata) = fs::metadata(original_path) {
        if let Ok(modified_time) = metadata.modified() {
            let dt: DateTime<Utc> = modified_time.into();
            let timestamp_str = dt.to_rfc3339();

            let mut hasher = Sha256::new();
            if let Some(path_str) = original_path.to_str() {
                hasher.update(path_str.as_bytes());
            } else {
                // Non-UTF8 paths are rare, but handle them somehow
                hasher.update(original_path.as_os_str().to_string_lossy().as_bytes());
            }
            hasher.update(timestamp_str.as_bytes());
            let result = hasher.finalize();
            let hash_str = hex::encode(result);

            let mut thumb_path = cache_dir.to_path_buf();
            thumb_path.push(format!("{}.webp", hash_str));
            return Some(thumb_path);
        }
    }
    None
}
