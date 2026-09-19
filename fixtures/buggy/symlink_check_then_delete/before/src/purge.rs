use std::fs;
use std::io;
use std::path::Path;

/// Empties the cache directory.
pub fn purge(dir: &Path) -> io::Result<()> {
    fs::remove_dir_all(dir)?;
    fs::create_dir(dir)
}
