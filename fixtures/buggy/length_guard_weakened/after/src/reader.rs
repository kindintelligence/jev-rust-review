use crate::frame::payload;

#[derive(Debug, PartialEq)]
pub enum FrameError {
    TooShort,
}

/// Extracts the payload of one frame received from the network.
pub fn read_frame(frame: &[u8]) -> Result<Vec<u8>, FrameError> {
    if frame.is_empty() {
        return Err(FrameError::TooShort);
    }
    Ok(payload(frame).to_vec())
}
