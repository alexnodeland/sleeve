//! Wiring the steps together.

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};

use crate::cli::Cli;
use crate::config::{Config, Format};
use crate::encode::{self, TrackTags};
use crate::naming::infer_tags;
use crate::probe::{self, Chapter, VideoInfo};
use crate::tools::{self, Tool};

/// What a run produced.
#[derive(Debug, Default)]
pub struct Outcome {
    pub album_dir: PathBuf,
    pub tracks: Vec<PathBuf>,
    pub cover: Option<PathBuf>,
    pub added_to_music: usize,
}

/// Album-level facts, after CLI overrides are applied to the inferred guess.
#[derive(Debug, Clone, PartialEq)]
pub struct AlbumMeta {
    pub artist: Option<String>,
    pub album: String,
    pub year: Option<u16>,
    pub genre: Option<String>,
    pub comment: Option<String>,
}

/// Combine what the video says with what the user overrode.
///
/// Split out and pure so the precedence is testable without a network.
pub fn album_meta(info: &VideoInfo, cli: &Cli, genre: Option<&str>) -> AlbumMeta {
    let inferred = infer_tags(&info.title);
    AlbumMeta {
        artist: cli.artist.clone().or(inferred.artist),
        album: cli.album.clone().unwrap_or(inferred.album),
        year: cli.year.or_else(|| info.year()),
        genre: cli.genre.clone().or_else(|| genre.map(str::to_string)),
        comment: cli.comment.clone(),
    }
}

/// Tags for every chapter, in order.
pub fn plan_tracks(chapters: &[Chapter], meta: &AlbumMeta) -> Vec<TrackTags> {
    let total = chapters.len();
    chapters
        .iter()
        .enumerate()
        .map(|(i, ch)| {
            encode::tags_for(
                ch,
                i + 1,
                total,
                meta.artist.as_deref(),
                &meta.album,
                meta.genre.as_deref(),
                meta.year,
                meta.comment.as_deref(),
            )
        })
        .collect()
}

/// Format `seconds` as `m:ss`, or `h:mm:ss` past an hour.
pub fn human_duration(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as u64;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// Run the whole pipeline.
pub fn run(cli: &Cli, cfg: &Config) -> Result<Outcome> {
    tools::check_available(&[Tool::YtDlp, Tool::Ffmpeg, Tool::Ffprobe])?;

    eprintln!("→ reading chapters");
    let info = probe::probe(&cli.url)?;
    let chapters = info.chapters().to_vec();
    probe::validate_chapters(&chapters)?;

    let meta = album_meta(&info, cli, cfg.genre.as_deref());
    let tracks = plan_tracks(&chapters, &meta);

    print_listing(&info, &chapters, &meta, cfg);

    if cli.list {
        return Ok(Outcome::default());
    }

    let album_dir = match cli.album_dir_name(&meta.album, meta.artist.as_deref()) {
        Some(name) => cfg.dest.join(name),
        None => cfg.dest.clone(),
    };

    if cli.dry_run {
        eprintln!("\ndry run — would write {} tracks to:", chapters.len());
        eprintln!("  {}", album_dir.display());
        return Ok(Outcome::default());
    }

    // Everything intermediate lives here and is removed on drop, including on
    // the error paths — a failed run should not leave a 500 MB download behind.
    let work = tempfile::tempdir().context("could not create a working directory")?;

    eprintln!("\n→ downloading audio");
    let source = download_audio(&cli.url, work.path())?;
    let source_codec = audio_codec(&source);

    let cover = if cli.no_cover {
        None
    } else {
        match build_cover(&cli.url, work.path()) {
            Ok(p) => Some(p),
            Err(e) => {
                // Art is a nice-to-have; losing it should not cost the rip.
                eprintln!("  ! cover art unavailable ({e}) — continuing without it");
                None
            }
        }
    };

    std::fs::create_dir_all(&album_dir)
        .with_context(|| format!("could not create {}", album_dir.display()))?;

    eprintln!("\n→ writing {} tracks", chapters.len());
    let mut written = Vec::with_capacity(chapters.len());
    for (i, (chapter, tags)) in chapters.iter().zip(&tracks).enumerate() {
        let out = encode::output_path(
            &album_dir,
            i + 1,
            chapters.len(),
            &chapter.title,
            cfg.format,
        );
        let args = encode::encode_args(
            &source,
            cover.as_deref(),
            chapter,
            tags,
            cfg.format,
            source_codec.as_deref(),
            &cfg.bitrate,
            &out,
        );
        tools::run_inherited(Tool::Ffmpeg, &args)
            .with_context(|| format!("could not write track {}: {}", i + 1, chapter.title))?;

        verify_duration(&out, chapter, i + 1);
        eprintln!(
            "  {:>2}. {:<44} {}",
            i + 1,
            truncate_display(&chapter.title, 44),
            human_duration(chapter.duration())
        );
        written.push(out);
    }

    // Formats that cannot embed art still deserve to keep it.
    let cover_out = match (&cover, cfg.format.supports_embedded_cover()) {
        (Some(c), false) => {
            let dest = album_dir.join("cover.jpg");
            std::fs::copy(c, &dest).ok().map(|_| dest)
        }
        (Some(_), true) => None,
        (None, _) => None,
    };

    if cfg.keep_full {
        let ext = source
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("audio");
        let dest = album_dir.join(format!("{}.{ext}", crate::naming::safe_stem(&info.title)));
        std::fs::copy(&source, &dest).context("could not keep the full-length audio")?;
        eprintln!("  + full-length audio kept");
    }

    let added = if cfg.add_to_music {
        add_to_music(cfg.format, &written)?
    } else {
        0
    };

    Ok(Outcome {
        album_dir,
        tracks: written,
        cover: cover_out,
        added_to_music: added,
    })
}

fn add_to_music(format: Format, tracks: &[PathBuf]) -> Result<usize> {
    if !format.importable_by_music_app() {
        eprintln!(
            "  ! Apple Music does not import .{} — skipping the import",
            format.extension()
        );
        return Ok(0);
    }
    let watched = crate::music::locate()?;
    let n = crate::music::add_tracks(&watched, tracks)?;
    eprintln!("\n→ handed {n} tracks to Apple Music");
    Ok(n)
}

fn print_listing(info: &VideoInfo, chapters: &[Chapter], meta: &AlbumMeta, cfg: &Config) {
    eprintln!("\n  {}", meta.album);
    if let Some(a) = &meta.artist {
        eprintln!("  {a}");
    }
    let total: f64 = info
        .duration
        .unwrap_or_else(|| chapters.last().map(|c| c.end_time).unwrap_or(0.0));
    eprintln!(
        "  {} tracks · {} · {}",
        chapters.len(),
        human_duration(total),
        cfg.format.extension()
    );
}

fn truncate_display(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let kept: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{kept}…")
}

/// Warn when a written track's real duration drifts from its chapter.
///
/// Not fatal: a final track can legitimately come up short when the uploader's
/// last chapter runs past the end of the audio. Worth saying out loud, though,
/// because it is also what a bad cut looks like.
fn verify_duration(path: &Path, chapter: &Chapter, index: usize) {
    let Some(actual) = tools::probe_f64(path, "format=duration") else {
        return;
    };
    let expected = chapter.duration();
    if (actual - expected).abs() > 1.5 {
        eprintln!(
            "  ! track {index} is {} but its chapter is {} — check the cut",
            human_duration(actual),
            human_duration(expected)
        );
    }
}

fn download_audio(url: &str, work: &Path) -> Result<PathBuf> {
    // The extension is whatever YouTube served, so the template leaves it to
    // yt-dlp and the file is found by prefix afterwards.
    let template = work.join("source.%(ext)s");
    let args = encode::download_args(url, &template.to_string_lossy());
    tools::run_inherited(Tool::YtDlp, &args).context("audio download failed")?;

    find_prefixed(work, "source.").context("yt-dlp reported success but produced no audio file")
}

fn build_cover(url: &str, work: &Path) -> Result<PathBuf> {
    let template = work.join("thumb.%(ext)s");
    let args = crate::cover::thumbnail_args(url, &template.to_string_lossy());
    tools::run_inherited(Tool::YtDlp, &args).context("thumbnail download failed")?;

    let thumb = find_prefixed(work, "thumb.").context("no thumbnail was written")?;
    crate::cover::build(&thumb, work)
}

/// The one file in `dir` whose name starts with `prefix`.
fn find_prefixed(dir: &Path, prefix: &str) -> Result<PathBuf> {
    let mut hits: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(prefix))
        })
        .collect();
    hits.sort();

    match hits.len() {
        0 => bail!("no file matching {prefix}* in {}", dir.display()),
        _ => Ok(hits.remove(0)),
    }
}

/// The codec of a media file's first audio stream.
fn audio_codec(path: &Path) -> Option<String> {
    let args = [
        "-v",
        "error",
        "-select_streams",
        "a:0",
        "-show_entries",
        "stream=codec_name",
        "-of",
        "default=noprint_wrappers=1:nokey=1",
        path.to_str()?,
    ];
    let out = tools::run_captured(Tool::Ffprobe, &args).ok()?;
    let codec = out.lines().next()?.trim().to_string();
    (!codec.is_empty()).then_some(codec)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn cli(args: &[&str]) -> Cli {
        Cli::parse_from(std::iter::once("sleeve").chain(args.iter().copied()))
    }

    fn info() -> VideoInfo {
        VideoInfo {
            title: "The Band - The Album [Full Album]".into(),
            uploader: Some("Uploader".into()),
            upload_date: Some("20260807".into()),
            duration: Some(300.0),
            chapters: Some(vec![
                Chapter {
                    title: "One".into(),
                    start_time: 0.0,
                    end_time: 100.0,
                },
                Chapter {
                    title: "Two".into(),
                    start_time: 100.0,
                    end_time: 300.0,
                },
            ]),
        }
    }

    #[test]
    fn metadata_is_inferred_when_nothing_is_overridden() {
        let meta = album_meta(&info(), &cli(&["u"]), None);
        assert_eq!(meta.artist.as_deref(), Some("The Band"));
        assert_eq!(meta.album, "The Album");
        assert_eq!(meta.year, Some(2026));
    }

    #[test]
    fn cli_overrides_beat_inference() {
        let cli = cli(&[
            "u",
            "--artist",
            "Someone Else",
            "--album",
            "Other",
            "--year",
            "1974",
        ]);
        let meta = album_meta(&info(), &cli, None);
        assert_eq!(meta.artist.as_deref(), Some("Someone Else"));
        assert_eq!(meta.album, "Other");
        assert_eq!(meta.year, Some(1974));
    }

    #[test]
    fn genre_falls_back_to_the_config_file() {
        let meta = album_meta(&info(), &cli(&["u"]), Some("Jazz"));
        assert_eq!(meta.genre.as_deref(), Some("Jazz"));

        let meta = album_meta(&info(), &cli(&["u", "--genre", "Rock"]), Some("Jazz"));
        assert_eq!(meta.genre.as_deref(), Some("Rock"));
    }

    #[test]
    fn every_chapter_becomes_a_numbered_track_sharing_one_album() {
        let info = info();
        let meta = album_meta(&info, &cli(&["u"]), None);
        let tracks = plan_tracks(info.chapters(), &meta);

        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0].track, 1);
        assert_eq!(tracks[1].track, 2);
        assert!(tracks.iter().all(|t| t.total == 2));
        assert!(tracks.iter().all(|t| t.album == "The Album"));
        assert_eq!(tracks[1].title, "Two");
    }

    #[test]
    fn durations_render_as_minutes_until_an_hour() {
        assert_eq!(human_duration(59.0), "0:59");
        assert_eq!(human_duration(60.0), "1:00");
        assert_eq!(human_duration(746.0), "12:26");
        assert_eq!(human_duration(3600.0), "1:00:00");
        assert_eq!(human_duration(8015.0), "2:13:35");
    }

    #[test]
    fn negative_durations_do_not_underflow() {
        assert_eq!(human_duration(-5.0), "0:00");
    }

    #[test]
    fn find_prefixed_picks_the_matching_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("source.webm"), b"x").unwrap();
        std::fs::write(dir.path().join("thumb.jpg"), b"y").unwrap();

        let found = find_prefixed(dir.path(), "source.").unwrap();
        assert_eq!(found.file_name().unwrap(), "source.webm");
    }

    #[test]
    fn find_prefixed_errors_when_nothing_matches() {
        let dir = tempfile::tempdir().unwrap();
        assert!(find_prefixed(dir.path(), "source.").is_err());
    }

    #[test]
    fn long_titles_are_ellipsised_for_the_listing() {
        let s = truncate_display(&"x".repeat(60), 44);
        assert_eq!(s.chars().count(), 44);
        assert!(s.ends_with('…'));
    }

    #[test]
    fn short_titles_are_left_alone() {
        assert_eq!(truncate_display("Red", 44), "Red");
    }
}
