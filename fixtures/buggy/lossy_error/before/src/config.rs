use std::path::Path;

#[derive(Debug)]
pub enum ConfigError {
    Io(std::io::Error),
    Parse(String),
}

pub fn load(path: &Path) -> Result<String, ConfigError> {
    let text = std::fs::read_to_string(path).map_err(ConfigError::Io)?;
    if text.trim().is_empty() {
        return Err(ConfigError::Parse("empty config".into()));
    }
    Ok(text)
}
