pub fn encode(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 4);
    let len = u32::try_from(payload.len()).expect("payload under 4 GiB");
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(payload);
    out
}
