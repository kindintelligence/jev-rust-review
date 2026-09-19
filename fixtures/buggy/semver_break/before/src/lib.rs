#[derive(Debug)]
pub struct ParseError;

#[derive(Debug, Default)]
pub struct Config {
    pub name: String,
    pub retries: u32,
}

pub fn parse(input: &str) -> Result<Config, ParseError> {
    let mut cfg = Config::default();
    for line in input.lines() {
        if let Some(v) = line.strip_prefix("name=") {
            cfg.name = v.to_string();
        }
    }
    Ok(cfg)
}
