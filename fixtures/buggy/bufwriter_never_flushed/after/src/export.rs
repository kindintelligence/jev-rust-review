use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;

pub struct Row {
    pub id: u32,
    pub name: String,
}

pub fn export(path: &Path, rows: &[Row]) -> io::Result<usize> {
    let mut out = BufWriter::new(File::create(path)?);
    for row in rows {
        writeln!(out, "{},{}", row.id, row.name)?;
    }
    Ok(rows.len())
}
