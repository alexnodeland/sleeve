#!/bin/sh
# sleeve installer.
#
#   curl -fsSL https://raw.githubusercontent.com/alexnodeland/sleeve/main/install.sh | sh
#
# Downloads the release binary for this platform, verifies its SHA-256 against
# the checksum published alongside it, and installs it. Override with:
#
#   SLEEVE_VERSION=v0.1.0   pin a version instead of taking the latest
#   SLEEVE_PREFIX=~/.local  install somewhere other than the default
#
# POSIX sh on purpose — this runs before the user has anything installed, so it
# cannot assume bash.

set -eu

REPO="alexnodeland/sleeve"
VERSION="${SLEEVE_VERSION:-latest}"

say() { printf '%s\n' "$*"; }
err() { printf 'error: %s\n' "$*" >&2; exit 1; }

need() {
    command -v "$1" >/dev/null 2>&1 || err "this installer needs '$1' on PATH"
}

need uname
need tar
need mktemp

# curl or wget, whichever is present.
if command -v curl >/dev/null 2>&1; then
    fetch() { curl -fsSL "$1" -o "$2"; }
elif command -v wget >/dev/null 2>&1; then
    fetch() { wget -qO "$2" "$1"; }
else
    err "this installer needs curl or wget"
fi

# ---------------------------------------------------------------- platform

os="$(uname -s)"
arch="$(uname -m)"

case "$os" in
    Darwin)
        # One universal binary covers both Apple silicon and Intel, so the
        # arch is deliberately not consulted here.
        asset="sleeve-macos-universal.tar.gz"
        ;;
    Linux)
        case "$arch" in
            x86_64 | amd64) asset="sleeve-linux-x86_64.tar.gz" ;;
            *) err "no prebuilt binary for Linux $arch — build from source: cargo install --path ." ;;
        esac
        ;;
    *)
        err "unsupported platform: $os (build from source: cargo install --path .)"
        ;;
esac

# ---------------------------------------------------------------- location

# Prefer a system-wide bin, but never silently ask for sudo: if it is not
# writable, fall back to a user-local prefix and say so.
if [ -n "${SLEEVE_PREFIX:-}" ]; then
    bindir="$SLEEVE_PREFIX/bin"
elif [ -w /usr/local/bin ] 2>/dev/null; then
    bindir="/usr/local/bin"
else
    bindir="$HOME/.local/bin"
    say "note: /usr/local/bin is not writable — installing to $bindir"
fi

mkdir -p "$bindir" || err "could not create $bindir"

# ---------------------------------------------------------------- download

if [ "$VERSION" = "latest" ]; then
    base="https://github.com/$REPO/releases/latest/download"
else
    base="https://github.com/$REPO/releases/download/$VERSION"
fi

tmp="$(mktemp -d)"
# shellcheck disable=SC2064  # expand $tmp now, not at trap time
trap "rm -rf '$tmp'" EXIT INT TERM

say "downloading $asset ($VERSION)"
fetch "$base/$asset" "$tmp/$asset" || err "could not download $base/$asset
If this is a 404, the release may not be published yet — check:
  https://github.com/$REPO/releases"

# ---------------------------------------------------------------- verify

if fetch "$base/$asset.sha256" "$tmp/$asset.sha256" 2>/dev/null; then
    if command -v shasum >/dev/null 2>&1; then
        sha="$(shasum -a 256 "$tmp/$asset" | awk '{print $1}')"
    elif command -v sha256sum >/dev/null 2>&1; then
        sha="$(sha256sum "$tmp/$asset" | awk '{print $1}')"
    else
        sha=""
        say "warning: no shasum/sha256sum available — skipping checksum verification"
    fi

    if [ -n "$sha" ]; then
        want="$(awk '{print $1}' "$tmp/$asset.sha256")"
        [ "$sha" = "$want" ] || err "checksum mismatch for $asset
  expected $want
  got      $sha"
        say "checksum ok"
    fi
else
    say "warning: no published checksum for $asset — skipping verification"
fi

# ---------------------------------------------------------------- install

tar -xzf "$tmp/$asset" -C "$tmp" || err "could not extract $asset"
[ -f "$tmp/sleeve" ] || err "$asset did not contain a 'sleeve' binary"

chmod +x "$tmp/sleeve"
mv "$tmp/sleeve" "$bindir/sleeve" || err "could not install to $bindir"

say "installed $bindir/sleeve"

# ---------------------------------------------------------------- follow-up

case ":$PATH:" in
    *":$bindir:"*) ;;
    *) say "note: $bindir is not on your PATH — add it to your shell profile" ;;
esac

missing=""
command -v yt-dlp >/dev/null 2>&1 || missing="yt-dlp"
command -v ffmpeg >/dev/null 2>&1 || missing="$missing ffmpeg"

if [ -n "$missing" ]; then
    say ""
    say "sleeve needs these at runtime and they are not installed:$missing"
    say "  brew install$missing"
fi
