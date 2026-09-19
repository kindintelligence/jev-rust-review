pub fn parse_port(s: &str) -> Result<u16, std::num::ParseIntError> {
    s.trim().parse()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_port() {
        let n = parse_port("8080").unwrap();
        assert_eq!(n, 8080);
        assert!(parse_port("x").is_err());
    }
}
