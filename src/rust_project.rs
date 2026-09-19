//! Deterministic project facts from Cargo manifests and file layout.

use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Known async runtimes, detected from dependencies. Never assumed.
const RUNTIMES: &[(&str, &str)] = &[
    ("tokio", "tokio"),
    ("async-std", "async-std"),
    ("smol", "smol"),
    ("embassy-executor", "embassy"),
    ("actix-rt", "actix-rt"),
    ("glommio", "glommio"),
    ("monoio", "monoio"),
    ("compio", "compio"),
];

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct CrateInfo {
    pub name: String,
    /// Repo-relative directory containing Cargo.toml ("." for the root).
    pub dir: String,
    pub edition: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rust_version: Option<String>,
    /// "lib", "bin", "lib+bin", "proc-macro", or "unknown".
    pub kind: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub async_runtimes: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub profiles: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<String>,
    /// Feature sets that look mutually exclusive (e.g. guarded by
    /// `compile_error!`). `--all-features` should not be used when non-empty.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub mutually_exclusive_features: Vec<Vec<String>>,
    pub has_build_rs: bool,
    /// Dependency names (normal, dev, build, target-specific), for detection.
    #[serde(skip)]
    pub dependencies: BTreeSet<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ProjectInfo {
    pub is_workspace: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub workspace_members: Vec<String>,
    pub crates: Vec<CrateInfo>,
    /// Policy and toolchain files that exist at the repo root.
    pub policy_files: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub toolchain: Option<String>,
    /// Facts about the diff under review, filled in by the pipeline.
    pub tests: TestFacts,
}

/// Whether the diff under review touches test code. This is a fact about the
/// diff, computed from file roles and `#[cfg(test)]`/`#[test]` spans, not a
/// question for Jev.
#[derive(Debug, Clone, Serialize, Default)]
pub struct TestFacts {
    pub diff_touches_tests: bool,
    /// `file:start-end` of changed units that are test code.
    pub test_units: Vec<String>,
    /// `file:start-end` of changed units that are not test code.
    pub non_test_units: Vec<String>,
}

const POLICY_FILES: &[&str] = &[
    "rust-toolchain",
    "rust-toolchain.toml",
    "clippy.toml",
    ".clippy.toml",
    "rustfmt.toml",
    ".rustfmt.toml",
    "deny.toml",
    "CLAUDE.md",
    "AGENTS.md",
    "README.md",
    ".cargo/config.toml",
];

/// The role a source file plays, which changes what is worth flagging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Library,
    Binary,
    Test,
    Example,
    Bench,
    BuildScript,
}

impl Role {
    pub fn describe(self) -> &'static str {
        match self {
            Role::Library => "library code",
            Role::Binary => "application (binary) code",
            Role::Test => "test code",
            Role::Example => "example code",
            Role::Bench => "benchmark code",
            Role::BuildScript => "build script",
        }
    }
}

impl ProjectInfo {
    /// Load facts for the repository at `root`. Only the manifests are read.
    pub fn load(root: &Path) -> ProjectInfo {
        let mut info = ProjectInfo {
            policy_files: POLICY_FILES
                .iter()
                .filter(|f| root.join(f).exists())
                .map(|f| f.to_string())
                .collect(),
            ..Default::default()
        };
        info.toolchain = read_toolchain(root);

        let root_manifest = read_toml(&root.join("Cargo.toml"));
        let mut dirs: Vec<PathBuf> = Vec::new();
        if let Some(m) = &root_manifest {
            if let Some(ws) = m.get("workspace").and_then(|w| w.as_table()) {
                info.is_workspace = true;
                for (rel, d) in workspace_member_dirs(root, ws) {
                    info.workspace_members.push(rel);
                    dirs.push(d);
                }
            }
            if m.get("package").is_some() && !dirs.iter().any(|d| d == root) {
                dirs.insert(0, root.to_path_buf());
            }
        }
        for d in dirs {
            if let Some(c) = load_crate(root, &d) {
                info.crates.push(c);
            }
        }
        info
    }

    /// The crate owning a repo-relative file: the deepest crate dir that
    /// prefixes it.
    pub fn crate_for(&self, file: &str) -> Option<&CrateInfo> {
        self.crates
            .iter()
            .filter(|c| c.dir == "." || file.starts_with(&format!("{}/", c.dir)))
            .max_by_key(|c| if c.dir == "." { 0 } else { c.dir.len() })
    }

    pub fn all_profiles(&self) -> BTreeSet<String> {
        self.crates
            .iter()
            .flat_map(|c| c.profiles.iter().cloned())
            .collect()
    }

    /// Suggested cargo commands, avoiding `--all-features` when features
    /// look mutually exclusive.
    pub fn cargo_commands(&self) -> (Vec<String>, Option<String>) {
        let ws = if self.is_workspace {
            " --workspace"
        } else {
            ""
        };
        let exclusive = self
            .crates
            .iter()
            .any(|c| !c.mutually_exclusive_features.is_empty());
        let note = if exclusive {
            Some("Some features look mutually exclusive (compile_error! guards); using default features only.".to_string())
        } else if self.crates.iter().any(|c| !c.features.is_empty()) {
            Some("Using default features. Feature-gated code is not compiled unless you pass --features.".to_string())
        } else {
            None
        };
        // Check and Clippy run inside the `cargo_diagnostics` tool, which
        // filters their output to the change. Tests are left to the caller:
        // a failing test has no line to filter by.
        (vec![format!("cargo test{ws} --no-fail-fast")], note)
    }
}

/// Decide a file's role from its path and, for inline test modules, the
/// line position inside the file.
pub fn role_for(file: &str, krate: Option<&CrateInfo>) -> Role {
    let rel = match krate {
        Some(c) if c.dir != "." => file.strip_prefix(&format!("{}/", c.dir)).unwrap_or(file),
        _ => file,
    };
    let first = rel.split('/').next().unwrap_or("");
    let name = rel.rsplit('/').next().unwrap_or(rel);
    match first {
        "tests" => Role::Test,
        "examples" => Role::Example,
        "benches" => Role::Bench,
        _ if rel == "build.rs" => Role::BuildScript,
        _ if name == "tests.rs" || name.ends_with("_test.rs") || name.ends_with("_tests.rs") => {
            Role::Test
        }
        _ if rel.starts_with("src/bin/") || rel == "src/main.rs" => Role::Binary,
        _ => match krate.map(|c| c.kind.as_str()) {
            Some("bin") => Role::Binary,
            _ => Role::Library,
        },
    }
}

/// Line numbers (1-based, inclusive) covered by `#[cfg(test)]` modules or
/// `#[test]` functions in a parsed file.
pub fn test_line_ranges(src: &str) -> Vec<(u32, u32)> {
    use syn::spanned::Spanned;
    let Ok(file) = syn::parse_file(src) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    fn is_test_attr(attrs: &[syn::Attribute]) -> bool {
        attrs.iter().any(|a| {
            let p = a.path();
            if p.is_ident("test") {
                return true;
            }
            if p.segments.len() == 2 && p.segments.last().is_some_and(|s| s.ident == "test") {
                // #[tokio::test], #[async_std::test], ...
                return true;
            }
            if p.is_ident("cfg")
                && let Ok(list) = a.meta.require_list()
            {
                return list
                    .tokens
                    .to_string()
                    .split_whitespace()
                    .any(|t| t == "test");
            }
            false
        })
    }
    fn walk(items: &[syn::Item], out: &mut Vec<(u32, u32)>) {
        for item in items {
            let attrs: &[syn::Attribute] = match item {
                syn::Item::Mod(m) => &m.attrs,
                syn::Item::Fn(f) => &f.attrs,
                syn::Item::Impl(i) => &i.attrs,
                _ => &[],
            };
            if is_test_attr(attrs) {
                let s = item.span();
                out.push((s.start().line as u32, s.end().line as u32));
            } else if let syn::Item::Mod(m) = item
                && let Some((_, inner)) = &m.content
            {
                walk(inner, out);
            }
        }
    }
    walk(&file.items, &mut out);
    out
}

/// Workspace member directories (repo-relative name, absolute path), with
/// `exclude` applied and duplicates removed.
fn workspace_member_dirs(root: &Path, ws: &toml::Table) -> Vec<(String, PathBuf)> {
    let excludes: Vec<String> = str_array(ws.get("exclude"));
    let mut out: Vec<(String, PathBuf)> = Vec::new();
    let candidates = str_array(ws.get("members"))
        .into_iter()
        .flat_map(|pat| expand_member(root, &pat));
    for d in candidates {
        let rel = rel_dir(root, &d);
        if !excludes.contains(&rel) && !out.iter().any(|(_, x)| x == &d) {
            out.push((rel, d));
        }
    }
    out
}

fn read_toml(p: &Path) -> Option<toml::Table> {
    std::fs::read_to_string(p).ok()?.parse::<toml::Table>().ok()
}

fn read_toolchain(root: &Path) -> Option<String> {
    if let Some(t) = read_toml(&root.join("rust-toolchain.toml")) {
        return t
            .get("toolchain")
            .and_then(|t| t.get("channel"))
            .and_then(|c| c.as_str())
            .map(str::to_string);
    }
    std::fs::read_to_string(root.join("rust-toolchain"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s.len() < 64)
}

fn str_array(v: Option<&toml::Value>) -> Vec<String> {
    v.and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn rel_dir(root: &Path, d: &Path) -> String {
    let r = d
        .strip_prefix(root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();
    if r.is_empty() { ".".into() } else { r }
}

/// Expand a workspace member pattern. Supports literal paths and a single
/// trailing `*` segment (`crates/*`), which covers nearly all workspaces.
fn expand_member(root: &Path, pat: &str) -> Vec<PathBuf> {
    if pat.contains("..") || Path::new(pat).is_absolute() {
        return Vec::new();
    }
    if let Some(prefix) = pat.strip_suffix("/*") {
        let dir = root.join(prefix);
        let mut v: Vec<PathBuf> = std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.join("Cargo.toml").is_file())
            .collect();
        v.sort();
        return v;
    }
    let p = root.join(pat);
    if p.join("Cargo.toml").is_file() {
        vec![p]
    } else {
        Vec::new()
    }
}

fn load_crate(root: &Path, dir: &Path) -> Option<CrateInfo> {
    let m = read_toml(&dir.join("Cargo.toml"))?;
    let pkg = m.get("package")?.as_table()?;
    let inherited = |key: &str| -> Option<String> {
        match pkg.get(key) {
            Some(toml::Value::String(s)) => Some(s.clone()),
            Some(toml::Value::Table(t))
                if t.get("workspace").and_then(|w| w.as_bool()) == Some(true) =>
            {
                read_toml(&root.join("Cargo.toml")).and_then(|r| {
                    r.get("workspace")?
                        .get("package")?
                        .get(key)?
                        .as_str()
                        .map(str::to_string)
                })
            }
            _ => None,
        }
    };
    let name = pkg.get("name")?.as_str()?.to_string();
    let edition = inherited("edition").unwrap_or_else(|| "2015".into());
    let rust_version = inherited("rust-version");

    let mut deps = BTreeSet::new();
    let mut collect = |t: Option<&toml::Value>| {
        if let Some(t) = t.and_then(|t| t.as_table()) {
            for (k, v) in t {
                // `foo = { package = "real-name" }` renames.
                let real = v
                    .get("package")
                    .and_then(|p| p.as_str())
                    .unwrap_or(k.as_str());
                deps.insert(real.to_string());
            }
        }
    };
    for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
        collect(m.get(section));
    }
    if let Some(targets) = m.get("target").and_then(|t| t.as_table()) {
        for (_, t) in targets {
            for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
                collect(t.get(section));
            }
        }
    }

    let has_lib = dir.join("src/lib.rs").exists() || m.get("lib").is_some();
    let proc_macro = m
        .get("lib")
        .and_then(|l| l.get("proc-macro"))
        .and_then(|v| v.as_bool())
        == Some(true);
    let has_bin =
        dir.join("src/main.rs").exists() || dir.join("src/bin").is_dir() || m.get("bin").is_some();
    let kind = match (proc_macro, has_lib, has_bin) {
        (true, _, _) => "proc-macro",
        (_, true, true) => "lib+bin",
        (_, true, false) => "lib",
        (_, false, true) => "bin",
        _ => "unknown",
    }
    .to_string();

    let features: Vec<String> = m
        .get("features")
        .and_then(|f| f.as_table())
        .map(|t| t.keys().filter(|k| *k != "default").cloned().collect())
        .unwrap_or_default();
    let mutually_exclusive_features = detect_exclusive_features(dir, &features);

    let async_runtimes = RUNTIMES
        .iter()
        .filter(|(dep, _)| deps.contains(*dep))
        .map(|(_, name)| name.to_string())
        .collect();
    let profiles = crate::questions::PROFILES
        .iter()
        .filter(|p| p.detect_crates.iter().any(|c| deps.contains(*c)))
        .map(|p| p.name.to_string())
        .collect();

    Some(CrateInfo {
        name,
        dir: rel_dir(root, dir),
        edition,
        rust_version,
        kind,
        async_runtimes,
        profiles,
        features,
        mutually_exclusive_features,
        has_build_rs: dir.join("build.rs").exists()
            || m.get("package").and_then(|p| p.get("build")).is_some(),
        dependencies: deps,
    })
}

/// Look for `compile_error!` guarded by `all(feature = "a", feature = "b")`
/// in the crate root: the conventional way to declare exclusive features.
fn detect_exclusive_features(dir: &Path, features: &[String]) -> Vec<Vec<String>> {
    if features.len() < 2 {
        return Vec::new();
    }
    let re = regex::Regex::new(r#"all\s*\(([^)]*)\)"#).expect("static regex");
    let feat = regex::Regex::new(r#"feature\s*=\s*"([^"]+)""#).expect("static regex");
    let mut out: BTreeMap<Vec<String>, ()> = BTreeMap::new();
    for root_file in ["src/lib.rs", "src/main.rs"] {
        let Ok(src) = std::fs::read_to_string(dir.join(root_file)) else {
            continue;
        };
        let lines: Vec<&str> = src.lines().collect();
        for (i, l) in lines.iter().enumerate() {
            if !l.contains("cfg") {
                continue;
            }
            let window = lines
                .iter()
                .skip(i)
                .take(3)
                .copied()
                .collect::<Vec<_>>()
                .join(" ");
            if !window.contains("compile_error!") {
                continue;
            }
            for c in re.captures_iter(l) {
                let set: Vec<String> = feat
                    .captures_iter(&c[1])
                    .map(|f| f[1].to_string())
                    .collect();
                if set.len() >= 2 {
                    out.insert(set, ());
                }
            }
        }
    }
    out.into_keys().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles() {
        assert_eq!(role_for("tests/it.rs", None), Role::Test);
        assert_eq!(role_for("examples/demo.rs", None), Role::Example);
        assert_eq!(role_for("benches/b.rs", None), Role::Bench);
        assert_eq!(role_for("build.rs", None), Role::BuildScript);
        assert_eq!(role_for("src/main.rs", None), Role::Binary);
        assert_eq!(role_for("src/bin/tool.rs", None), Role::Binary);
        assert_eq!(role_for("src/lib.rs", None), Role::Library);
        assert_eq!(role_for("src/foo/tests.rs", None), Role::Test);
    }

    #[test]
    fn test_ranges_found() {
        let src = "fn a() {}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() {}\n}\n";
        assert_eq!(test_line_ranges(src), vec![(3, 7)]);
        let src2 = "#[tokio::test]\nasync fn t() {\n}\nfn b() {}\n";
        assert_eq!(test_line_ranges(src2), vec![(1, 3)]);
        assert!(test_line_ranges("not rust {{{").is_empty());
    }
}
