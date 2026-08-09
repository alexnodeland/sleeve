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

# Run against a real video, writing to ./scratch (gitignored).
#
# NOT part of `just ci` — it needs the network and the two external tools.
# The URL is any chapter-marked video; override with `just smoke URL=...`.
URL := "https://youtu.be/CHANGE_ME"
smoke:
    cargo run -- --list "{{URL}}"
    cargo run -- --dry-run --dest ./scratch "{{URL}}"

# Everything CI runs, in CI order
ci: fmt-check clippy test deny doc msrv
    @echo "ci suite green"
