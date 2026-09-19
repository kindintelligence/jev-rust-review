pub fn parse_port(s: &str) -> Result<u16, std::num::ParseIntError> {
    s.trim().parse()
}
