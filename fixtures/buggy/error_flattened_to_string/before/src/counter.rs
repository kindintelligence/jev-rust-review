use crate::error::StoreError;

pub fn parse_counter(text: &str) -> Result<u64, StoreError> {
    text.trim()
        .parse()
        .map_err(|e| StoreError::Parse(format!("{text:?} is not a counter: {e}")))
}
