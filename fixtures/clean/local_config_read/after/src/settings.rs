use std::io;
use std::path::Path;

pub fn parse_port(text: &str) -> Option<u16> {
    text.lines()
        .find_map(|line| line.strip_prefix("port="))
        .and_then(|value| value.trim().parse().ok())
}

pub fn default_path() -> &'static Path {
    Path::new("/etc/relay/relay.conf")
}

/// The port from the operator's config file, if it sets one.
pub fn load_port() -> io::Result<Option<u16>> {
    let text = std::fs::read_to_string(default_path())?;
    Ok(parse_port(&text))
}
