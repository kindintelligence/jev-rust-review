use std::path::PathBuf;
use std::time::Duration;

pub struct Report {
    pub body: String,
}

pub async fn load_report(dir: PathBuf, name: &str) -> std::io::Result<Report> {
    let path = dir.join(name);
    let body = std::fs::read_to_string(&path)?;
    if body.is_empty() {
        // Give the writer a moment to finish, then retry once.
        std::thread::sleep(Duration::from_millis(200));
        let body = std::fs::read_to_string(&path)?;
        return Ok(Report { body });
    }
    Ok(Report { body })
}
