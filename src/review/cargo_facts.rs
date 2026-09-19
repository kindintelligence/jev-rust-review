//! Deterministic facts from `Cargo.toml` and `Cargo.lock` diffs.

use super::types::*;
use crate::git::{Git, ResolvedScope};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

fn dep_spec(v: &toml::Value) -> String {
    match v {
        toml::Value::String(s) => s.clone(),
        toml::Value::Table(t) => {
            let mut parts = Vec::new();
            for k in [
                "version",
                "git",
                "branch",
                "tag",
                "rev",
                "path",
                "workspace",
            ] {
                if let Some(x) = t.get(k) {
                    parts.push(format!("{k}={}", x.to_string().trim_matches('"')));
                }
            }
            parts.join(" ")
        }
        other => other.to_string(),
    }
}

fn dep_tables(m: &toml::Table) -> BTreeMap<(String, String), toml::Value> {
    let mut out = BTreeMap::new();
    let mut add = |section: String, t: Option<&toml::Value>| {
        if let Some(t) = t.and_then(|t| t.as_table()) {
            for (k, v) in t {
                out.insert((section.clone(), k.clone()), v.clone());
            }
        }
    };
    for s in ["dependencies", "dev-dependencies", "build-dependencies"] {
        add(s.into(), m.get(s));
    }
    if let Some(ws) = m.get("workspace").and_then(|w| w.as_table()) {
        add("workspace.dependencies".into(), ws.get("dependencies"));
    }
    if let Some(targets) = m.get("target").and_then(|t| t.as_table()) {
        for (tname, t) in targets {
            for s in ["dependencies", "dev-dependencies", "build-dependencies"] {
                add(format!("target.{tname}.{s}"), t.get(s));
            }
        }
    }
    out
}

fn risky(name: &str, v: &toml::Value) -> Option<String> {
    let t = v.as_table();
    if t.is_some_and(|t| t.contains_key("git")) {
        let pinned = t.is_some_and(|t| t.contains_key("rev") || t.contains_key("tag"));
        return Some(format!(
            "{name}: git dependency{}",
            if pinned {
                ""
            } else {
                " without a pinned rev or tag"
            }
        ));
    }
    let version = match v {
        toml::Value::String(s) => Some(s.as_str()),
        toml::Value::Table(t) => t.get("version").and_then(|v| v.as_str()),
        _ => None,
    };
    if version.is_some_and(|s| s.trim() == "*") {
        return Some(format!("{name}: wildcard version \"*\""));
    }
    if let Some(p) = t.and_then(|t| t.get("path")).and_then(|p| p.as_str())
        && (p.starts_with("..") || Path::new(p).is_absolute())
    {
        return Some(format!("{name}: path dependency outside this crate ({p})"));
    }
    None
}

pub fn compare_manifests(file: &str, old: Option<&str>, new: Option<&str>) -> ManifestFacts {
    let parse = |s: Option<&str>| {
        s.and_then(|s| s.parse::<toml::Table>().ok())
            .unwrap_or_default()
    };
    let (o, n) = (parse(old), parse(new));
    let (od, nd) = (dep_tables(&o), dep_tables(&n));
    let mut f = ManifestFacts {
        file: file.into(),
        ..Default::default()
    };
    for ((section, name), v) in &nd {
        match od.get(&(section.clone(), name.clone())) {
            None => {
                f.added_dependencies.push(DepChange {
                    name: name.clone(),
                    section: section.clone(),
                    from: None,
                    to: Some(dep_spec(v)),
                });
                if let Some(w) = risky(name, v) {
                    f.warnings.push(w);
                }
            }
            Some(ov) if ov != v => {
                f.changed_dependencies.push(DepChange {
                    name: name.clone(),
                    section: section.clone(),
                    from: Some(dep_spec(ov)),
                    to: Some(dep_spec(v)),
                });
                if let Some(w) = risky(name, v) {
                    f.warnings.push(w);
                }
            }
            _ => {}
        }
    }
    for ((section, name), v) in &od {
        if !nd.contains_key(&(section.clone(), name.clone())) {
            f.removed_dependencies.push(DepChange {
                name: name.clone(),
                section: section.clone(),
                from: Some(dep_spec(v)),
                to: None,
            });
        }
    }
    let feats = |m: &toml::Table| {
        m.get("features")
            .and_then(|f| f.as_table())
            .cloned()
            .unwrap_or_default()
    };
    let (of, nf) = (feats(&o), feats(&n));
    f.removed_features = of
        .keys()
        .filter(|k| !nf.contains_key(*k))
        .cloned()
        .collect();
    f.default_features_changed = !of.is_empty() && of.get("default") != nf.get("default");
    let pkg = |m: &toml::Table, k: &str| {
        m.get("package")
            .and_then(|p| p.get(k))
            .map(|v| v.to_string().trim_matches('"').to_string())
    };
    for (key, slot) in [("edition", 0), ("rust-version", 1)] {
        let (a, b) = (pkg(&o, key), pkg(&n, key));
        if a != b && (a.is_some() || b.is_some()) && old.is_some() {
            let change = Some((a.unwrap_or_default(), b.unwrap_or_default()));
            if slot == 0 {
                f.edition_change = change;
            } else {
                f.rust_version_change = change;
            }
        }
    }
    let build_added = pkg(&n, "build").is_some() && pkg(&o, "build").is_none();
    if build_added {
        f.warnings
            .push("package.build: new build script configured".into());
    }
    let pm = |m: &toml::Table| {
        m.get("lib")
            .and_then(|l| l.get("proc-macro"))
            .and_then(|v| v.as_bool())
            == Some(true)
    };
    if pm(&n) && !pm(&o) {
        f.warnings.push("lib.proc-macro enabled".into());
    }
    f
}

pub(super) fn manifest_facts(git: &Git, rs: &ResolvedScope, path: &str) -> ManifestFacts {
    let old = git.read_old(rs, path).ok().flatten();
    let new = git.read_new(&rs.new_side, path).ok().flatten();
    compare_manifests(path, old.as_deref(), new.as_deref())
}

pub fn compare_lockfiles(file: &str, old: Option<&str>, new: Option<&str>) -> LockfileFacts {
    let pkgs = |s: Option<&str>| -> BTreeMap<String, BTreeSet<String>> {
        let mut m: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        if let Some(t) = s.and_then(|s| s.parse::<toml::Table>().ok())
            && let Some(arr) = t.get("package").and_then(|p| p.as_array())
        {
            for p in arr {
                if let (Some(n), Some(v)) = (
                    p.get("name").and_then(|x| x.as_str()),
                    p.get("version").and_then(|x| x.as_str()),
                ) {
                    m.entry(n.to_string()).or_default().insert(v.to_string());
                }
            }
        }
        m
    };
    let (o, n) = (pkgs(old), pkgs(new));
    let mut f = LockfileFacts {
        file: file.into(),
        ..Default::default()
    };
    for (name, vs) in &n {
        match o.get(name) {
            None => {
                f.packages_added += 1;
                f.sample.push(format!(
                    "+{name} {}",
                    vs.iter().cloned().collect::<Vec<_>>().join(",")
                ));
            }
            Some(ovs) if ovs != vs => {
                f.packages_updated += 1;
                f.sample.push(format!(
                    "~{name} {} -> {}",
                    ovs.iter().cloned().collect::<Vec<_>>().join(","),
                    vs.iter().cloned().collect::<Vec<_>>().join(",")
                ));
            }
            _ => {}
        }
    }
    for name in o.keys() {
        if !n.contains_key(name) {
            f.packages_removed += 1;
            f.sample.push(format!("-{name}"));
        }
    }
    f.sample.truncate(20);
    f
}

pub(super) fn lockfile_facts(git: &Git, rs: &ResolvedScope, path: &str) -> LockfileFacts {
    let old = git.read_old(rs, path).ok().flatten();
    let new = git.read_new(&rs.new_side, path).ok().flatten();
    compare_lockfiles(path, old.as_deref(), new.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_comparison() {
        let old = "[package]\nname='a'\nedition='2021'\n[dependencies]\nserde='1'\nold='0.1'\n[features]\ndefault=['x']\nx=[]\ny=[]\n";
        let new = "[package]\nname='a'\nedition='2024'\nbuild='build.rs'\n[dependencies]\nserde='2'\nnewgit={git='https://example.com/x'}\nstar='*'\n[features]\ndefault=[]\nx=[]\n";
        let f = compare_manifests("Cargo.toml", Some(old), Some(new));
        let names = |v: &Vec<DepChange>| v.iter().map(|d| d.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(&f.added_dependencies), vec!["newgit", "star"]);
        assert_eq!(names(&f.removed_dependencies), vec!["old"]);
        assert_eq!(names(&f.changed_dependencies), vec!["serde"]);
        assert_eq!(f.removed_features, vec!["y"]);
        assert!(f.default_features_changed);
        assert_eq!(f.edition_change, Some(("2021".into(), "2024".into())));
        assert!(
            f.warnings
                .iter()
                .any(|w| w.contains("git dependency without a pinned"))
        );
        assert!(f.warnings.iter().any(|w| w.contains("wildcard")));
        assert!(f.warnings.iter().any(|w| w.contains("build script")));
    }

    #[test]
    fn lockfile_comparison() {
        let old = "version = 4\n[[package]]\nname='a'\nversion='1.0.0'\n[[package]]\nname='b'\nversion='1.0.0'\n";
        let new = "version = 4\n[[package]]\nname='a'\nversion='1.1.0'\n[[package]]\nname='c'\nversion='0.1.0'\n";
        let f = compare_lockfiles("Cargo.lock", Some(old), Some(new));
        assert_eq!(
            (f.packages_added, f.packages_removed, f.packages_updated),
            (1, 1, 1)
        );
    }
}
