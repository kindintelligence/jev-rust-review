#!/usr/bin/env bash
# Smoke test: build the server and drive a real MCP stdio session
# (initialize, tools/list, dry-run tools/call) against a temporary git repo.
# Needs: cargo, git, python3. Never needs TYPESAFE_API_KEY.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PY="$(command -v python3 || command -v python || true)"
[ -n "$PY" ] || { echo "smoke: python3 is required to drive the stdio session" >&2; exit 1; }

if [ -z "${JEV_RUST_REVIEW_BIN:-}" ]; then
  cargo build --quiet --manifest-path "$ROOT/Cargo.toml"
  BIN="$ROOT/target/debug/jev-rust-review"
  [ -x "$BIN" ] || BIN="$BIN.exe"
else
  BIN="$JEV_RUST_REVIEW_BIN"
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
REPO="$TMP/repo"
mkdir -p "$REPO/src"
cd "$REPO"
git init -q
git config user.email smoke@example.com
git config user.name smoke
cat > Cargo.toml <<'TOML'
[package]
name = "smoke"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { version = "1", features = ["full"] }
TOML
cat > src/lib.rs <<'RS'
use std::sync::Mutex;

pub struct Cache {
    value: Mutex<u64>,
}

impl Cache {
    pub async fn refresh(&self) -> u64 {
        *self.value.lock().unwrap()
    }
}
RS
git add -A && git commit -qm init
cat > src/lib.rs <<'RS'
use std::sync::Mutex;

pub struct Cache {
    value: Mutex<u64>,
}

impl Cache {
    pub async fn refresh(&self) -> u64 {
        let mut guard = self.value.lock().unwrap();
        *guard = fetch().await;
        *guard
    }
}

async fn fetch() -> u64 {
    42
}
RS
printf 'SECRET_TOKEN=abc\n' > .env

export TYPESAFE_API_KEY="smoke-test-key-must-never-appear"
export JEV_RUST_REVIEW_API_URL="http://127.0.0.1:9"   # unroutable: nothing may be sent
unset CLAUDE_PROJECT_DIR || true

"$PY" - "$BIN" "$REPO" <<'PYDRIVER'
import json, subprocess, sys, threading, queue

bin_path, repo = sys.argv[1], sys.argv[2]
proc = subprocess.Popen([bin_path], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                        stderr=subprocess.PIPE, cwd=repo)
lines = queue.Queue()
raw_stdout = []

def pump():
    for line in proc.stdout:
        raw_stdout.append(line)
        lines.put(line)
threading.Thread(target=pump, daemon=True).start()

def send(msg):
    proc.stdin.write((json.dumps(msg) + "\n").encode())
    proc.stdin.flush()

def wait_for(id_, timeout=60):
    while True:
        line = lines.get(timeout=timeout)
        msg = json.loads(line)  # every stdout line must be JSON
        assert msg.get("jsonrpc") == "2.0", msg
        if msg.get("id") == id_:
            return msg

def fail(why):
    proc.kill()
    sys.stderr.write("smoke FAILED: " + why + "\n")
    sys.stderr.write(proc.stderr.read().decode(errors="replace")[-2000:])
    sys.exit(1)

send({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
    "protocolVersion": "2025-06-18", "capabilities": {},
    "clientInfo": {"name": "smoke", "version": "0"}}})
init = wait_for(1)
if init["result"]["serverInfo"]["name"] != "jev-rust-review":
    fail("unexpected serverInfo: %r" % init["result"]["serverInfo"])
send({"jsonrpc": "2.0", "method": "notifications/initialized"})

send({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})
tools = {t["name"] for t in wait_for(2)["result"]["tools"]}
if tools != {"evaluate_rust_changes", "verify_rust_findings"}:
    fail("unexpected tools: %r" % tools)

send({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {
    "name": "evaluate_rust_changes",
    "arguments": {"repo_path": repo, "dry_run": True}}})
res = wait_for(3)["result"]
if res.get("isError"):
    fail("evaluate returned an error: %r" % res)
out = json.loads(res["content"][0]["text"])
checks = [
    (out["status"] == "dry_run", "status is dry_run"),
    (len(out["units"]) >= 1, "at least one unit"),
    (out["units"][0]["file"] == "src/lib.rs", "unit file is src/lib.rs"),
    (out["units"][0]["changed_lines"][0][0] >= 9, "changed lines computed from the diff"),
    ("tokio" in out["active_profiles"], "tokio profile detected"),
    (out["project"]["crates"][0]["async_runtimes"] == ["tokio"], "runtime detected"),
    (any(s["file"] == ".env" for s in out["skipped"]), ".env skipped"),
    (len(out["payloads"]) == len(out["units"]), "one payload per unit"),
    ("fetch().await" in out["payloads"][0]["body"]["state"]["code"], "payload contains the code"),
    ("async.guard_across_await" in out["payloads"][0]["body"]["questions"], "guard question gated in"),
    ("tokio.runtime_nesting" not in out["payloads"][0]["body"]["questions"], "irrelevant tokio question gated out"),
    (out["usage"]["requests"] == 0, "nothing sent"),
]
for ok, what in checks:
    if not ok:
        fail(what + "\n" + json.dumps(out, indent=1)[:4000])

send({"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {
    "name": "verify_rust_findings",
    "arguments": {"repo_path": repo, "dry_run": True, "findings": [{
        "dimension": "async", "file": "src/lib.rs", "start_line": 9, "end_line": 11,
        "claim": "A std::sync::MutexGuard is held across the await of fetch() in refresh.",
        "severity": "high"}]}}})
v = json.loads(wait_for(4)["result"]["content"][0]["text"])
if v["status"] != "dry_run" or ">        *guard = fetch().await;" not in v["payloads"][0]["body"]["state"]["code"]:
    fail("verify dry run: " + json.dumps(v)[:3000])

send({"jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": {
    "name": "evaluate_rust_changes",
    "arguments": {"repo_path": repo, "scope": "--output=/tmp/pwned"}}})
bad = wait_for(5)["result"]
if not bad.get("isError") or "must not begin with '-'" not in bad["content"][0]["text"]:
    fail("option-injection scope was not rejected: %r" % bad)

proc.stdin.close()
proc.wait(timeout=20)
blob = b"".join(raw_stdout)
if b"smoke-test-key-must-never-appear" in blob:
    fail("API key leaked to stdout")
print("smoke OK: initialize, tools/list, evaluate (dry run), verify (dry run), scope injection rejected")
PYDRIVER
