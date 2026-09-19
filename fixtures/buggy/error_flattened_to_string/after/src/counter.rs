use crate::error::StoreError;
use std::path::Path;

pub fn parse_counter(text: &str) -> Result<u64, StoreError> {
    text.trim()
        .parse()
        .map_err(|e| StoreError::Parse(format!("{text:?} is not a counter: {e}")))
}

pub fn load_counter(path: &Path) -> Result<u64, StoreError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| StoreError::Parse(format!("cannot load counter: {e}")))?;
    parse_counter(&text)
}
