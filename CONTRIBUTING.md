# Contributing to sleeve

Issues and small PRs are welcome. Coordinate before starting anything large.

## Dev setup

> **CI:** every push and PR runs [`.github/workflows/ci.yml`](.github/workflows/ci.yml).
> `just ci` runs the same suite locally, in the same order, so nothing lands
> without it being green.

Everything goes through [`just`](https://github.com/casey/just) — run `just`
with no arguments to list the recipes.

```sh
git clone https://github.com/alexnodeland/sleeve
cd sleeve
just setup     # installs git hooks (lefthook), checks for the runtime tools
just ci        # fmt, clippy, test, deny, doc
```

Required to develop: stable Rust (MSRV is the `rust-version` in `Cargo.toml`),
[lefthook](https://github.com/evilmartians/lefthook), and
[cargo-deny](https://github.com/EmbarkStudios/cargo-deny).

Required to *run* the tool, but **not** to run the tests: `yt-dlp` and
`ffmpeg`.

## The tests are hermetic, and that is load-bearing

The suite never spawns yt-dlp or ffmpeg and never touches the network. It runs
in well under a second, which is what makes it usable as a pre-push hook.

This is possible because everything that shells out is split in two:

- a **pure `*_args()` function** that builds the argv and returns it, and
- a **thin runner** in `tools.rs` that executes it.

All the interesting knowledge lives in the arg builders — which ffmpeg flags
prevent a track inheriting the source's chapter list, why `-ss` goes before
`-i`, why cropdetect needs `-loop 1`. Tests assert on the returned argv.

**When you add behaviour that shells out, put the knowledge in an arg builder
and test the argv.** A change that can only be verified by running ffmpeg
against a real download is a change that will not be verified.

Several tests exist specifically to pin down a bug that was shipped once and
was invisible in the output — `every_track_drops_the_sources_chapter_list` and
`cropdetect_loops_the_still_so_it_has_frames_to_converge_on` are both of that
kind. Do not delete a test like that because it looks like it is asserting the
obvious; the comment above it says what went wrong.

### Testing against a real video

`just smoke` runs `--list` and `--dry-run` against a real URL. It is not part
of `just ci` because it needs the network and both external tools.

```sh
just smoke                          # the default URL
just smoke URL=https://youtu.be/x   # your own
```

## Ground rules

- **Conventional Commits**, enforced by a `commit-msg` hook:
  `type(scope): description`. Types: `feat`, `fix`, `docs`, `style`,
  `refactor`, `perf`, `test`, `chore`, `ci`, `build`, `revert`.
- **Comments explain why, not what.** The code already says what it does. A
  comment earns its place by recording the reason a non-obvious choice was
  made — usually the failure that would happen without it.
- **`just ci` green before you push.** The hooks run fmt and clippy on commit
  and the tests on push, so this mostly happens on its own.
- **Update `CHANGELOG.md`** under an `## [Unreleased]` heading for anything
  user-visible.

## Releasing

1. Move `## [Unreleased]` to `## [X.Y.Z] - YYYY-MM-DD` in `CHANGELOG.md`.
2. Bump `version` in `Cargo.toml`, and commit both.
3. Tag `vX.Y.Z` and push the tag.

The release workflow verifies that the tag, `Cargo.toml`, and the leading
`CHANGELOG.md` section all agree — a mismatch fails the build rather than
shipping the previous version's notes. It then builds a macOS universal binary
and a Linux x86_64 binary and opens a **draft** release. Review it, then
publish.
