use std::path::Path;

#[derive(Debug)]
pub enum ConfigError {
    Invalid,
}

pub fn load(path: &Path) -> Result<toml::Table, ConfigError> {
    let text = std::fs::read_to_string(path).map_err(|_| ConfigError::Invalid)?;
    let table = text
        .parse::<toml::Table>()
        .map_err(|_| ConfigError::Invalid)?;
    Ok(table)
}
