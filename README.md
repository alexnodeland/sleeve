# sleeve

[![CI](https://img.shields.io/github/actions/workflow/status/alexnodeland/sleeve/ci.yml?branch=main&label=CI)](https://github.com/alexnodeland/sleeve/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/alexnodeland/sleeve?include_prereleases&label=release)](https://github.com/alexnodeland/sleeve/releases)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#license)
![Platform](https://img.shields.io/badge/platform-macOS%20%C2%B7%20Linux-informational)
![Built with](https://img.shields.io/badge/Rust%20%C2%B7%20yt--dlp%20%C2%B7%20ffmpeg-555)

Full albums and concert recordings get posted to YouTube as one long video with
the track listing in the chapter markers. **sleeve** turns that back into an
album: one tagged audio file per chapter, square cover art from the video
thumbnail, and — on macOS — straight into Apple Music.

```sh
sleeve https://youtu.be/VIDEO_ID
```

```
→ reading chapters

  Live at the Lantern Rooms
  Nightjar Quartet
  8 tracks · 1:04:12 · m4a

→ downloading audio
→ writing 8 tracks
   1. Copper Wire                                  6:41
   2. Marsh Light                                  9:03
   3. Slow Ascent                                  7:28
   …
   8. Undertow                                    11:16
```

That is the whole interface for the common case. The artist, album, and year
are inferred from the video title and upload date; every one of them can be
overridden.

## Features

| | |
|---|---|
| ✂️ **Chapter-accurate splitting** | One track per chapter, cut sample-accurately — no keyframe drift |
| 🏷️ **Complete tags** | Title, artist, album artist, album, date, genre, track *n/total*, disc, comment |
| 🖼️ **Cover art** | The video thumbnail, letterbox/pillarbox detected and removed, padded square, embedded in every track |
| 🎧 **Formats** | `m4a` (AAC), `mp3`, or `opus` — the last stream-copied from YouTube's own audio, so it is not re-encoded at all |
| 🍎 **Apple Music** | `--music` hands the finished tracks to your library, no Automation permission needed |
| 📁 **Configurable output** | Destination, format, bitrate, and genre via flags, `SLEEVE_*` env vars, or a config file |
| 🔍 **Look before you leap** | `--list` prints the track listing without downloading; `--dry-run` shows exactly what would be written |

## Install

sleeve needs **yt-dlp** and **ffmpeg** at runtime; it checks for both at
startup rather than failing after a long download.

```sh
brew install yt-dlp ffmpeg
```

Then grab a binary from [releases](https://github.com/alexnodeland/sleeve/releases),
or build from source:

```sh
git clone https://github.com/alexnodeland/sleeve
cd sleeve
cargo install --path .
```

## Usage

```sh
sleeve <URL> [OPTIONS]
```

| Flag | Meaning |
|---|---|
| `-d`, `--dest <PATH>` | Where to write the album folder (default: your Desktop) |
| `-f`, `--format <FMT>` | `m4a` (default), `mp3`, or `opus` |
| `-b`, `--bitrate <RATE>` | e.g. `256k` (default: 256k for m4a, 320k for mp3) |
| `--artist <NAME>` | Override the inferred artist |
| `--album <TITLE>` | Override the inferred album |
| `--year <YEAR>` | Override the upload year |
| `--genre <GENRE>` | Genre tag |
| `--comment <TEXT>` | Comment written to every track |
| `-m`, `--music` | Also add the tracks to Apple Music (macOS) |
| `--keep-full` | Keep the un-split full-length audio too |
| `--no-cover` | Skip cover art |
| `--flat` | Write into the destination directly, no album subfolder |
| `-n`, `--dry-run` | Show what would be written, then stop |
| `-l`, `--list` | Print the track listing and exit — no download |

Tracks land in an album folder named `Artist - Album`, with filenames like
`03 - Slow Ascent.m4a` — zero-padded to the width of the track count so they sort
correctly everywhere.

### Configuration

Settings resolve in this order: **flag → `SLEEVE_*` env var → config file →
built-in default.** The config file lives at `~/.config/sleeve/config.toml`:

```toml
dest = "/Users/you/Music/Rips"
format = "m4a"
bitrate = "256k"
genre = "Live"
add-to-music = true
keep-full = false
```

Unknown keys are an error, not a silent no-op — a typo'd setting that does
nothing is indistinguishable from a setting that does not work.

### Apple Music

`--music` copies the finished tracks into the folder Music watches for
automatic imports. It deliberately does **not** use AppleScript: scripting
Music requires the Automation entitlement, and when that has been denied the
script hangs instead of failing. The watched folder needs no permission at all.

Tracks are *copied*, never moved, so your output directory still has them
afterward. Opus is skipped here — Apple Music will not import it.

## How it works

```
yt-dlp --dump-single-json   →  chapters, title, upload date
yt-dlp -f bestaudio         →  ONE download of the source audio
yt-dlp --write-thumbnail    →  cover art  →  cropdetect → crop → pad square
ffmpeg (per chapter)        →  cut · re-encode · tag · embed art
```

Three details are the difference between output that looks right and output
that *is* right:

- **Every track is cut from the source download**, not from an intermediate, so
  it is one encode away from YouTube's audio rather than two.
- **Every track drops the source's chapter list** (`-map_chapters -1`). Without
  that, track 3 of an 8-track album carries all 8 chapter markers and players
  present it as the entire album — the durations look correct while the file
  lies about what it is.
- **Cover art is padded to square, not cropped to it.** Padding costs
  background; cropping costs the picture, and on a title card it crops the
  text. Letterboxing already in the thumbnail is detected and removed first.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). `just` lists the recipes; `just ci` runs
everything CI runs.

The test suite is hermetic — it never spawns yt-dlp or ffmpeg and never touches
the network. Every command the tool builds is a pure `*_args()` function
returning the argv, and the tests assert on that, which is what makes the
ffmpeg knowledge testable at all.

## A note on what you rip

sleeve is a tool for making your own copies of things playable and organized.
Whether a given video is yours to download is between you, the uploader, and
the rights holder — the track titles and artwork it writes come from the
uploader's own chapter list and thumbnail, so a rip is a personal-library
artifact, not a released album, and it will not match anything in a store.

## License

MIT OR Apache-2.0, at your option.
