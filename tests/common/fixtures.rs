//! The fixture corpus: each fixture is two snapshots of a small buildable
//! crate (`before/` and `after/`) and a `fixture.toml` that says what the
//! change does wrong, or, for a clean fixture, which wrong claim it baits.

use super::{TestRepo, fixtures_dir};
use jev_rust_review::review::Finding;
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct FixtureToml {
    pub description: String,
    /// The file the claim is about, relative to the crate root.
    pub file: String,
    #[serde(default)]
    pub expected_dimensions: Vec<String>,
    /// The label for "deeper than the compiler": whether `cargo check` plus
    /// the extended Clippy set reports this bug on the claim's lines. The
    /// three-mode eval checks the label against what the tools really say.
    #[serde(default)]
    pub tool_catches: bool,
    /// Where a modelled bug comes from: an issue, a pull request, or an
    /// advisory. The code is always a fresh minimal reproduction.
    #[serde(default)]
    pub source: Option<String>,
    pub claim: ClaimToml,
}

#[derive(Debug, Deserialize)]
pub struct ClaimToml {
    pub dimension: String,
    pub start: String,
    pub end: String,
    pub text: String,
    pub severity: String,
}

pub struct Fixture {
    pub kind: &'static str,
    pub name: String,
    pub dir: PathBuf,
    pub spec: FixtureToml,
}

fn copy_tree(from: &Path, to: &Path) {
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            std::fs::create_dir_all(&target).unwrap();
            copy_tree(&entry.path(), &target);
        } else {
            // Fixtures are stored with LF; keep that whatever git checked out.
            let text = std::fs::read_to_string(entry.path())
                .unwrap()
                .replace("\r\n", "\n");
            std::fs::write(target, text).unwrap();
        }
    }
}

impl Fixture {
    pub fn buggy(&self) -> bool {
        self.kind == "buggy"
    }

    pub fn read_after(&self, rel: &str) -> String {
        std::fs::read_to_string(self.dir.join("after").join(rel))
            .unwrap()
            .replace("\r\n", "\n")
    }

    /// A repository with `before/` committed and `after/` in the working
    /// tree, so the default scope reviews exactly the fixture's change.
    pub fn repo(&self) -> TestRepo {
        let r = TestRepo::new();
        r.write(".gitignore", "/target\nCargo.lock\n");
        copy_tree(&self.dir.join("before"), &r.path());
        r.commit_all("before");
        // Replace the tree, so a file the change deletes is deleted.
        for entry in std::fs::read_dir(r.path()).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name();
            if name == ".git" || name == ".gitignore" {
                continue;
            }
            if entry.file_type().unwrap().is_dir() {
                std::fs::remove_dir_all(entry.path()).unwrap();
            } else {
                std::fs::remove_file(entry.path()).unwrap();
            }
        }
        copy_tree(&self.dir.join("after"), &r.path());
        r
    }

    /// The claim's line range, located by substring so fixtures stay
    /// editable without renumbering.
    pub fn claim_lines(&self) -> (u32, u32) {
        let after = self.read_after(&self.spec.file);
        let lines: Vec<&str> = after.lines().collect();
        let start = lines
            .iter()
            .position(|l| l.contains(&self.spec.claim.start))
            .unwrap_or_else(|| panic!("{}: start marker not found", self.name));
        let end = lines
            .iter()
            .skip(start)
            .position(|l| l.contains(&self.spec.claim.end))
            .map(|i| i + start)
            .unwrap_or_else(|| panic!("{}: end marker not found", self.name));
        (start as u32 + 1, end as u32 + 1)
    }

    pub fn finding(&self) -> Finding {
        let (s, e) = self.claim_lines();
        Finding {
            id: Some(self.name.clone()),
            dimension: self.spec.claim.dimension.clone(),
            file: self.spec.file.clone(),
            start_line: s,
            end_line: e,
            claim: self.spec.claim.text.clone(),
            severity: self.spec.claim.severity.clone(),
        }
    }

    pub fn recording_path(&self) -> PathBuf {
        self.dir.join("recorded.json")
    }
}

pub fn load_fixtures() -> Vec<Fixture> {
    let mut out = Vec::new();
    for kind in ["buggy", "clean"] {
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(fixtures_dir().join(kind))
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.join("fixture.toml").is_file())
            .collect();
        dirs.sort();
        for dir in dirs {
            let spec: FixtureToml =
                toml::from_str(&std::fs::read_to_string(dir.join("fixture.toml")).unwrap())
                    .unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
            out.push(Fixture {
                kind,
                name: dir.file_name().unwrap().to_string_lossy().into_owned(),
                dir,
                spec,
            });
        }
    }
    assert!(out.len() >= 15, "corpus unexpectedly small");
    out
}
