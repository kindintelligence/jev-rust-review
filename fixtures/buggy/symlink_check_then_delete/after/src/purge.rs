use std::fs;
use std::io;
use std::path::Path;

/// Empties the cache directory. The directory itself stays in place, so a
/// watcher on it stays valid.
pub fn purge(dir: &Path) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        let meta = fs::symlink_metadata(&path)?;
        if meta.file_type().is_symlink() || meta.is_file() {
            fs::remove_file(&path)?;
        } else {
            purge(&path)?;
            fs::remove_dir(&path)?;
        }
    }
    Ok(())
}
