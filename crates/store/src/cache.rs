//! Best-effort limits for reproducible cache files, never saved user data.
use std::{fs, path::Path, time::SystemTime};

/// Record a cache hit without rewriting the payload on every read.
pub fn touch(path: &Path) {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return;
    };
    if !meta.is_file()
        || meta
            .modified()
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age.as_secs() < 3600)
    {
        return;
    }
    if let Ok(file) = fs::File::open(path) {
        let _ = file.set_modified(SystemTime::now());
    }
}

/// Remove oldest recognized regular files until both limits hold. The newly
/// requested entry is retained even if it alone exceeds the byte budget.
/// Unknown names, temporary files, directories and symlinks are left alone.
pub fn trim(
    dir: &Path,
    keep: &Path,
    max_files: usize,
    max_bytes: u64,
    recognized: impl Fn(&str) -> bool,
) {
    let Ok(files) = fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = files
        .flatten()
        .filter_map(|entry| {
            if !entry.file_type().ok()?.is_file() || !recognized(entry.file_name().to_str()?) {
                return None;
            }
            let meta = entry.metadata().ok()?;
            Some((meta.modified().ok()?, entry.path(), meta.len()))
        })
        .collect();
    let mut count = entries.len();
    let mut bytes: u64 = entries.iter().map(|(_, _, size)| size).sum();
    if count <= max_files && bytes <= max_bytes {
        return;
    }
    entries.sort_unstable();
    for (_, path, size) in entries {
        if count <= max_files && bytes <= max_bytes {
            break;
        }
        if path != keep && fs::remove_file(path).is_ok() {
            count -= 1;
            bytes -= size;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn limits_count_and_bytes_without_removing_active_or_unrelated_files() {
        let dir = tempfile::tempdir().unwrap();
        for n in 0..6 {
            let path = dir.path().join(format!("{n}.cache"));
            fs::write(&path, [0; 10]).unwrap();
            fs::File::open(path)
                .unwrap()
                .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(n))
                .unwrap();
        }
        let keep = dir.path().join("0.cache");
        let unrelated = dir.path().join("notes.json");
        fs::write(&unrelated, "user data").unwrap();
        fs::create_dir(dir.path().join("directory.cache")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&unrelated, dir.path().join("link.cache")).unwrap();
        trim(dir.path(), &keep, 4, 30, |s| s.ends_with(".cache"));
        assert!(keep.exists());
        assert!(!dir.path().join("3.cache").exists());
        assert!(dir.path().join("4.cache").exists());
        assert!(dir.path().join("5.cache").exists());
        assert_eq!(fs::read_to_string(unrelated).unwrap(), "user data");
        assert!(dir.path().join("directory.cache").is_dir());
        #[cfg(unix)]
        assert!(dir.path().join("link.cache").is_symlink());
        trim(dir.path(), &keep, 1, 5, |s| s.ends_with(".cache"));
        assert!(keep.exists(), "an oversized active export must stay usable");
        assert!(!dir.path().join("5.cache").exists());
    }

    #[test]
    fn recent_hits_are_not_rewritten_and_old_hits_become_recent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hit");
        fs::write(&path, "cached").unwrap();
        fs::File::open(&path)
            .unwrap()
            .set_modified(SystemTime::UNIX_EPOCH)
            .unwrap();
        touch(&path);
        let touched = fs::metadata(&path).unwrap().modified().unwrap();
        assert!(touched.elapsed().unwrap().as_secs() < 10);
        touch(&path);
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), touched);
        assert_eq!(fs::read_to_string(path).unwrap(), "cached");
    }
}
