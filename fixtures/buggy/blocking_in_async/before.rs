use std::path::PathBuf;

pub struct Report {
    pub body: String,
}

pub async fn load_report(dir: PathBuf, name: &str) -> std::io::Result<Report> {
    let body = tokio::fs::read_to_string(dir.join(name)).await?;
    Ok(Report { body })
}
