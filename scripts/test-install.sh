#!/usr/bin/env bash
# Fresh-install test for scripts/launch.sh. Simulates a new user: a pristine
# copy of the plugin (no target/), an empty CLAUDE_PLUGIN_DATA, and no
# jev-rust-review binary on PATH. Checks:
#   A. source build: launcher reports "still compiling" within the startup
#      budget, the detached build finishes, and the next launch serves MCP;
#   B. download: a (simulated) release asset with a matching SHA256SUMS is
#      installed and served;
#   C. tampered checksum: the asset is refused.
# Needs: git, cargo, python3, tar, sha256sum or shasum.
set -euo pipefail

SRC="$(cd "$(dirname "$0")/.." && pwd)"
PY="$(command -v python3 || command -v python)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
VERSION="$(sed -n 's/^version = "\(.*\)"$/\1/p' "$SRC/Cargo.toml" | head -n 1)"
EXE=""
case "$(uname -s)" in MINGW* | MSYS* | CYGWIN*) EXE=".exe" ;; esac

fail() {
    echo "test-install FAILED: $*" >&2
    exit 1
}

# Pristine plugin copy: tracked and untracked-but-not-ignored files only.
PLUGIN="$WORK/plugin"
mkdir -p "$PLUGIN"
(cd "$SRC" && git ls-files -co --exclude-standard -z | xargs -0 tar -cf -) | tar -xf - -C "$PLUGIN"
[ ! -e "$PLUGIN/target" ] || fail "plugin copy contains target/"

# A PATH without any jev-rust-review binary.
CLEAN_PATH="$(printf '%s' "$PATH" | tr ':' '\n' | while read -r d; do
    [ -n "$d" ] && [ ! -x "$d/jev-rust-review$EXE" ] && printf '%s:' "$d"
done)"

# Drive one MCP initialize through the launcher; succeed if the server answers.
handshake() {
    CLAUDE_PLUGIN_ROOT="$PLUGIN" CLAUDE_PLUGIN_DATA="$1" PATH="$CLEAN_PATH" \
        JEV_RUST_REVIEW_NO_DOWNLOAD="${2:-1}" JEV_RUST_REVIEW_RELEASE_URL="${3:-}" \
        "$PY" - "$PLUGIN/scripts/launch.sh" <<'PYEOF'
import json, subprocess, sys
p = subprocess.Popen(["sh", sys.argv[1]], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                     stderr=subprocess.PIPE)
msg = {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
    "protocolVersion": "2025-06-18", "capabilities": {},
    "clientInfo": {"name": "test-install", "version": "0"}}}
p.stdin.write((json.dumps(msg) + "\n").encode()); p.stdin.flush()
line = p.stdout.readline()
p.stdin.close()
err = p.stderr.read().decode(errors="replace")
p.wait(timeout=30)
sys.stderr.write(err)
if not line:
    sys.exit("no response from server")
r = json.loads(line)
assert r["result"]["serverInfo"]["name"] == "jev-rust-review", r
print("handshake ok")
PYEOF
}

# ---- A: build from source with an empty data dir ------------------------------
DATA_A="$WORK/data-a"
set +e
MSG="$(CLAUDE_PLUGIN_ROOT="$PLUGIN" CLAUDE_PLUGIN_DATA="$DATA_A" PATH="$CLEAN_PATH" \
    JEV_RUST_REVIEW_NO_DOWNLOAD=1 JEV_RUST_REVIEW_BUILD_WAIT=3 \
    sh "$PLUGIN/scripts/launch.sh" </dev/null 2>&1 >/dev/null)"
CODE=$?
set -e
echo "A1: $MSG"
[ "$CODE" -ne 0 ] || fail "A: expected a non-zero exit while compiling"
echo "$MSG" | grep -q "still compiling" || fail "A: expected the 'still compiling' message"
BIN_A="$DATA_A/bin/$VERSION/jev-rust-review$EXE"
for _ in $(seq 1 900); do
    [ -x "$BIN_A" ] && [ ! -d "$DATA_A/build-$VERSION.lock" ] && break
    sleep 1
done
[ -x "$BIN_A" ] || { cat "$DATA_A/build-$VERSION.log" >&2 || true; fail "A: background build did not install a binary"; }
handshake "$DATA_A" || fail "A: server did not come up from the cached build"
echo "A: source build path OK"

# ---- B: verified download from a simulated release -------------------------------
case "$(uname -s)-$(uname -m)" in
    Linux-x86_64 | Linux-amd64) TRIPLE=x86_64-unknown-linux-musl ;;
    Linux-aarch64 | Linux-arm64) TRIPLE=aarch64-unknown-linux-musl ;;
    Darwin-arm64) TRIPLE=aarch64-apple-darwin ;;
    Darwin-x86_64) TRIPLE=x86_64-apple-darwin ;;
    *) TRIPLE=x86_64-pc-windows-msvc ;;
esac
REL="$WORK/release"
mkdir -p "$REL/stage"
cp "$BIN_A" "$REL/stage/jev-rust-review$EXE"
ASSET="jev-rust-review-$TRIPLE.tar.gz"
tar -czf "$REL/$ASSET" -C "$REL/stage" "jev-rust-review$EXE"
if command -v sha256sum >/dev/null 2>&1; then
    (cd "$REL" && sha256sum "$ASSET" >SHA256SUMS)
else
    (cd "$REL" && shasum -a 256 "$ASSET" >SHA256SUMS)
fi
REL_URL="file://$REL"
case "$REL" in [A-Za-z]:*) REL_URL="file:///$REL" ;; esac
handshake "$WORK/data-b" 0 "$REL_URL" 2>"$WORK/b.err" || { cat "$WORK/b.err" >&2; fail "B: download path"; }
grep -q "checksum verified" "$WORK/b.err" || fail "B: expected checksum verification"
echo "B: verified download path OK"

# ---- C: tampered checksum is refused --------------------------------------------
sed 's/^[0-9a-f]\{8\}/00000000/' "$REL/SHA256SUMS" >"$REL/SHA256SUMS.bad" && mv "$REL/SHA256SUMS.bad" "$REL/SHA256SUMS"
NOCARGO_PATH="$(printf '%s' "$CLEAN_PATH" | tr ':' '\n' | while read -r d; do
    [ -n "$d" ] && [ ! -x "$d/cargo$EXE" ] && printf '%s:' "$d"
done)"
set +e
MSG="$(CLAUDE_PLUGIN_ROOT="$PLUGIN" CLAUDE_PLUGIN_DATA="$WORK/data-c" PATH="$NOCARGO_PATH" \
    JEV_RUST_REVIEW_RELEASE_URL="$REL_URL" sh "$PLUGIN/scripts/launch.sh" </dev/null 2>&1 >/dev/null)"
CODE=$?
set -e
echo "C: $MSG"
[ "$CODE" -ne 0 ] || fail "C: tampered asset was accepted"
echo "$MSG" | grep -q "checksum verification failed" || fail "C: expected a checksum failure message"
[ ! -e "$WORK/data-c/bin/$VERSION/jev-rust-review$EXE" ] || fail "C: tampered binary was installed"
echo "C: tampered checksum refused OK"
echo "test-install OK"
