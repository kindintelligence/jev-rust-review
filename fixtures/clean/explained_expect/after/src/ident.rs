use regex::Regex;
use std::sync::LazyLock;

static IDENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[A-Za-z0-9_]+$")
        .expect("IDENT is a valid regex literal")
});

pub fn is_ident(s: &str) -> bool {
    IDENT.is_match(s)
}
