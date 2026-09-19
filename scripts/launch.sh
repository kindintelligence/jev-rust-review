#!/bin/sh
# Launch the jev-rust-review MCP server for Claude Code (or any MCP client).
#
# stdout belongs to the MCP protocol, so this script never writes to it:
# every message goes to stderr. It execs the first binary that matches the
# plugin version, in this order:
#
#   1. $JEV_RUST_REVIEW_BIN (manual installs)
#   2. the cached binary in $CLAUDE_PLUGIN_DATA/bin/<version>/
#   3. `jev-rust-review` on PATH (e.g. from `cargo install --git ...`)
#   4. $CLAUDE_PLUGIN_ROOT/target/release/jev-rust-review (local development)
#   5. a prebuilt release asset, verified against the release's SHA256SUMS
#   6. a local `cargo build --release --locked`, in the background; waits up
#      to $JEV_RUST_REVIEW_BUILD_WAIT seconds (default 20, below Claude
#      Code's 30 s MCP startup timeout)
#
# `launch.sh --install` does the same work in the foreground (waiting for a
# source build as long as it takes), prints nothing to stdout, and exits
# instead of starting the server.
set -eu

ROOT="${CLAUDE_PLUGIN_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
REPO="kindintelligence/jev-rust-review"
VERSION="$(sed -n 's/^version = "\(.*\)"$/\1/p' "$ROOT/Cargo.toml" | head -n 1)"
DATA="${CLAUDE_PLUGIN_DATA:-${XDG_CACHE_HOME:-$HOME/.cache}/jev-rust-review}"
INSTALL_ONLY=0
[ "${1:-}" = "--install" ] && INSTALL_ONLY=1

EXE=""
case "$(uname -s)" in MINGW* | MSYS* | CYGWIN*) EXE=".exe" ;; esac
BIN_DIR="$DATA/bin/$VERSION"
BIN="$BIN_DIR/jev-rust-review$EXE"
LOG="$DATA/build-$VERSION.log"
LOCK="$DATA/build-$VERSION.lock"

say() { printf 'jev-rust-review: %s\n' "$*" >&2; }
die() {
    say "$*"
    exit 1
}

[ -n "$VERSION" ] || die "cannot read the version from $ROOT/Cargo.toml"

# A binary is usable only if it reports exactly this plugin's version.
version_ok() {
    [ -f "$1" ] && [ -x "$1" ] && [ "$("$1" --version 2>/dev/null || true)" = "jev-rust-review $VERSION" ]
}

run() {
    if [ "$INSTALL_ONLY" = 1 ]; then
        say "ready: $1"
        exit 0
    fi
    exec "$1"
}

install_bin() {
    mkdir -p "$BIN_DIR"
    tmp="$BIN.tmp.$$"
    cp "$1" "$tmp" && chmod +x "$tmp" && mv -f "$tmp" "$BIN"
}

build() {
    if cargo build --release --locked --manifest-path "$ROOT/Cargo.toml" \
        --target-dir "$DATA/build" >"$LOG" 2>&1; then
        install_bin "$DATA/build/release/jev-rust-review$EXE"
    fi
    rmdir "$LOCK" 2>/dev/null || true
}

# Internal: the detached background build started by step 6. The parent
# holds the lock directory; the worker removes it when done.
if [ "${1:-}" = "--build-worker" ]; then
    build
    exit 0
fi

# 1. Explicit override.
if [ -n "${JEV_RUST_REVIEW_BIN:-}" ]; then
    [ -x "$JEV_RUST_REVIEW_BIN" ] || die "JEV_RUST_REVIEW_BIN=$JEV_RUST_REVIEW_BIN is not an executable file"
    run "$JEV_RUST_REVIEW_BIN"
fi

# 2. Cached binary for this version.
version_ok "$BIN" && run "$BIN"

# 3. A matching binary on PATH.
if onpath="$(command -v "jev-rust-review$EXE" 2>/dev/null)" && version_ok "$onpath"; then
    run "$onpath"
fi

# 4. A local release build (plugin loaded with --plugin-dir from a checkout).
if version_ok "$ROOT/target/release/jev-rust-review$EXE"; then
    install_bin "$ROOT/target/release/jev-rust-review$EXE"
    run "$BIN"
fi

# 5. Prebuilt release asset, verified before it is cached.
target_triple() {
    arch="$(uname -m)"
    case "$arch" in
        x86_64 | amd64) arch=x86_64 ;;
        arm64 | aarch64) arch=aarch64 ;;
        *) return 1 ;;
    esac
    case "$(uname -s)" in
        Linux) echo "$arch-unknown-linux-musl" ;;
        Darwin) echo "$arch-apple-darwin" ;;
        MINGW* | MSYS* | CYGWIN*) echo "$arch-pc-windows-msvc" ;;
        *) return 1 ;;
    esac
}

sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d ' ' -f 1
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | cut -d ' ' -f 1
    else
        return 1
    fi
}

fetch() {
    # Only https by default; file:// is allowed when a release URL override
    # is set, which the tests use to simulate a release.
    protos='=https'
    [ -n "${JEV_RUST_REVIEW_RELEASE_URL:-}" ] && protos='=https,file'
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL --proto "$protos" --tlsv1.2 --retry 2 --connect-timeout 10 --max-time 60 -o "$2" "$1"
    elif command -v wget >/dev/null 2>&1; then
        wget -q --timeout=60 -O "$2" "$1"
    else
        return 1
    fi
}

download() {
    [ "${JEV_RUST_REVIEW_NO_DOWNLOAD:-0}" = 1 ] && return 1
    triple="$(target_triple)" || return 1
    base="${JEV_RUST_REVIEW_RELEASE_URL:-https://github.com/$REPO/releases/download/v$VERSION}"
    asset="jev-rust-review-$triple.tar.gz"
    tmpd="$(mktemp -d "${TMPDIR:-/tmp}/jev-rust-review.XXXXXX")" || return 1
    if ! fetch "$base/$asset" "$tmpd/$asset" || ! fetch "$base/SHA256SUMS" "$tmpd/SHA256SUMS"; then
        rm -rf "$tmpd"
        return 1
    fi
    want="$(awk -v a="$asset" '$2 == a || $2 == "*" a { print $1 }' "$tmpd/SHA256SUMS" | head -n 1)"
    got="$(sha256_of "$tmpd/$asset" || true)"
    if [ -z "$want" ] || [ "$want" != "$got" ]; then
        say "checksum verification failed for $asset; refusing to use it"
        rm -rf "$tmpd"
        return 1
    fi
    ok=1
    tar -xzf "$tmpd/$asset" -C "$tmpd" && version_ok "$tmpd/jev-rust-review$EXE" &&
        install_bin "$tmpd/jev-rust-review$EXE" && ok=0
    rm -rf "$tmpd"
    return "$ok"
}

if download; then
    say "installed prebuilt binary for $VERSION (checksum verified)"
    run "$BIN"
fi

# 6. Build from source.
command -v cargo >/dev/null 2>&1 || die "no prebuilt binary could be installed and cargo is not available. Install Rust from https://rustup.rs, or install manually with: cargo install --locked --git https://github.com/$REPO --tag v$VERSION (then restart, or set JEV_RUST_REVIEW_BIN to the binary)."
mkdir -p "$DATA"

# A lock older than 60 minutes belongs to a build that died.
if [ -d "$LOCK" ] && [ -n "$(find "$LOCK" -maxdepth 0 -mmin +60 2>/dev/null)" ]; then
    rmdir "$LOCK" 2>/dev/null || true
fi

if mkdir "$LOCK" 2>/dev/null; then
    say "building $VERSION from source; the first build takes a few minutes. Log: $LOG"
    if [ "$INSTALL_ONLY" = 1 ]; then
        build
    else
        # Detach so the build survives this launcher exiting on timeout.
        if command -v setsid >/dev/null 2>&1; then
            setsid sh "$ROOT/scripts/launch.sh" --build-worker </dev/null >/dev/null 2>&1 &
        else
            nohup sh "$ROOT/scripts/launch.sh" --build-worker </dev/null >/dev/null 2>&1 &
        fi
    fi
fi

waited=0
limit="${JEV_RUST_REVIEW_BUILD_WAIT:-20}"
[ "$INSTALL_ONLY" = 1 ] && limit=100000
while [ "$waited" -lt "$limit" ]; do
    version_ok "$BIN" && run "$BIN"
    [ -d "$LOCK" ] || break
    sleep 1
    waited=$((waited + 1))
done
version_ok "$BIN" && run "$BIN"

if [ -d "$LOCK" ]; then
    die "still compiling from source in the background (log: $LOG). Run /mcp and reconnect the 'jev' server when it finishes, or run: sh \"$ROOT/scripts/launch.sh\" --install"
fi
die "building from source failed; see $LOG. To install manually: cargo install --locked --git https://github.com/$REPO --tag v$VERSION, then set JEV_RUST_REVIEW_BIN to the installed binary."
