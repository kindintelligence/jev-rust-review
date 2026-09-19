#[derive(Debug)]
pub struct ParseError;

#[derive(Debug, Default)]
pub struct Config {
    pub retries: u32,
}

pub fn parse(input: &str, strict: bool) -> Result<Config, ParseError> {
    let mut cfg = Config::default();
    for line in input.lines() {
        if let Some(v) = line.strip_prefix("retries=") {
            cfg.retries = v.parse().map_err(|_| ParseError)?;
        } else if strict {
            return Err(ParseError);
        }
    }
    Ok(cfg)
}
