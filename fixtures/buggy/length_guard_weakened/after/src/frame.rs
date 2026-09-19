pub const HEADER: usize = 4;
pub const TRAILER: usize = 2;

/// The bytes between the header and the checksum trailer. The caller must
/// have checked that the frame holds both.
pub fn payload(frame: &[u8]) -> &[u8] {
    &frame[HEADER..frame.len() - TRAILER]
}
