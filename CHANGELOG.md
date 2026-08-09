# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `install.sh`, so the tool can be installed with a single `curl` line. It
  detects the platform, verifies the published SHA-256 before installing, and
  falls back to `~/.local/bin` rather than silently escalating to `sudo`.
- Illustrated README: hero, pipeline diagram, and a diagram of how cover art is
  squared.

### Fixed

- Replaced two let-chains, which are unstable before Rust 1.88 and so broke the
  build on the declared 1.85 MSRV.
- `just msrv` now exists and runs as part of `just ci`. The local gate did not
  mirror the CI gate, which is how the above reached CI in the first place.

## [0.1.0] - 2026-08-08

First release.

### Added

- Split a chapter-marked video into one tagged audio track per chapter, cut
  from a single download of the source audio.
- Cover art from the video thumbnail: letterboxing and pillarboxing detected
  and removed, then padded — not cropped — to square and embedded in every
  track.
- Tag set per track: title, artist, album artist, album, date, genre,
  track *n/total*, disc, and comment. Artist, album, and year are inferred from
  the video title and upload date, and each can be overridden.
- Output formats `m4a` (AAC), `mp3`, and `opus`. Opus is stream-copied from
  YouTube's own audio when the source is already Opus, so it is not re-encoded.
- `--music` adds finished tracks to Apple Music via the watched import folder,
  which needs no Automation entitlement.
- Configuration by flag, `SLEEVE_*` environment variable, or
  `~/.config/sleeve/config.toml`, in that precedence order. Unknown config keys
  are rejected rather than ignored.
- `--list` and `--dry-run` to inspect a video without downloading it.
- Startup check for yt-dlp and ffmpeg that reports every missing tool at once,
  before anything is downloaded.

[0.1.0]: https://github.com/alexnodeland/sleeve/releases/tag/v0.1.0
