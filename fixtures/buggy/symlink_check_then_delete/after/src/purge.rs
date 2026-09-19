use std::fs;
use std::io;
use std::path::Path;

/// Empties the cache directory, keeping `.lock` files in place.
pub fn purge(dir: &Path) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().is_some_and(|ext| ext == "lock") {
            continue;
        }
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
