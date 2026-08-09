# sleeve — DX front door. `just` lists the recipes; `just ci` is the gate.
#
# Green here means green in CI: the `ci` recipe at the bottom runs exactly what
# .github/workflows/ci.yml runs, in the same order.

# List available recipes
default:
    @just --list --unsorted

# First-time setup: install git hooks, verify the runtime tools exist
setup:
    lefthook install
    @command -v cargo-deny >/dev/null || echo "warning: cargo-deny not found — install with: cargo install cargo-deny"
    @command -v yt-dlp >/dev/null || echo "warning: yt-dlp not found — sleeve needs it at runtime: brew install yt-dlp"
    @command -v ffmpeg >/dev/null || echo "warning: ffmpeg not found — sleeve needs it at runtime: brew install ffmpeg"
    @echo "setup complete — run 'just ci' to verify"

# Build the binary
build:
    cargo build

# Release build (what the release workflow ships)
build-release:
    cargo build --release

# The test suite — hermetic: no network, no yt-dlp, no ffmpeg.
#
# Everything that shells out is split into a pure `*_args()` builder and a thin
# runner, and the tests assert on the argv. That keeps the suite fast and
# offline, and it is why the ffmpeg knowledge is testable at all.
test:
    cargo test

# Format all Rust code
fmt:
    cargo fmt --all

# Check formatting without writing
fmt-check:
    cargo fmt --all -- --check

# Clippy with warnings denied — the lint floor
clippy:
    cargo clippy --all-targets -- -D warnings

# fmt-check + clippy
lint: fmt-check clippy

# Rustdoc with warnings denied
doc:
    RUSTDOCFLAGS="-D warnings" cargo doc --no-deps

# Supply-chain gate: advisories, license allowlist, duplicate/source bans
deny:
    cargo deny check

# The MSRV promised by `rust-version` in Cargo.toml. Moving it means moving it
# in three places: here, Cargo.toml, and the msrv job in ci.yml.
MSRV := "1.85"

# Compile under the oldest supported Rust.
#
# RUSTC is pinned to the toolchain's own binary on purpose. If rust is also
# installed via Homebrew, plain `rustup run {{MSRV}} cargo check` will happily
# invoke the Homebrew rustc from PATH instead — the check then passes against
# current stable while reporting the pinned version, which is worse than not
# running it at all. That exact trap let a let-chain (unstable before 1.88)
# reach CI.
msrv:
    rustup toolchain install {{MSRV}} --profile minimal
    RUSTC="$(rustup run {{MSRV}} rustc --print sysroot)/bin/rustc" \
      rustup run {{MSRV}} cargo check --all-targets

# Run against a real chapter-marked video, writing to ./scratch (gitignored).
#
#   just smoke https://...        # one-off
#   SLEEVE_SMOKE_URL=https://...  # or set it once in your shell
#
# NOT part of `just ci` — it needs the network and both external tools.
#
# No URL is committed here, deliberately. A default would aim every
# contributor's downloader at some third party's video, and any link would
# eventually rot into a failure that looks like a bug in sleeve. There is also
# no stable, openly-licensed, chapter-marked video to point at: the obvious
# candidates (Blender's CC-BY open movies, public-domain archive.org items)
# carry no chapter markers, which is the one thing this test needs.
smoke url=env_var_or_default("SLEEVE_SMOKE_URL", ""):
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -z "{{url}}" ]; then
      echo "just smoke needs the URL of a chapter-marked video." >&2
      echo >&2
      echo "  just smoke https://..." >&2
      echo "  SLEEVE_SMOKE_URL=https://... just smoke" >&2
      echo >&2
      echo "None is committed on purpose — point it at something you have the" >&2
      echo "right to download. The CI suite is hermetic and needs no URL." >&2
      exit 2
    fi
    cargo run -- --list "{{url}}"
    cargo run -- --dry-run --dest ./scratch "{{url}}"

# Everything CI runs, in CI order
ci: fmt-check clippy test deny doc msrv
    @echo "ci suite green"
