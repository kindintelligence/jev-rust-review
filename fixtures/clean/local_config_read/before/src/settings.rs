use std::path::Path;

pub fn parse_port(text: &str) -> Option<u16> {
    text.lines()
        .find_map(|line| line.strip_prefix("port="))
        .and_then(|value| value.trim().parse().ok())
}

pub fn default_path() -> &'static Path {
    Path::new("/etc/relay/relay.conf")
}
