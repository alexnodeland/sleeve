//! Asking yt-dlp what a video contains, without downloading it.
//!
//! `--dump-single-json` is one network round trip and returns everything the
//! rest of the pipeline needs: the chapter list, a title to infer tags from,
//! and the thumbnail URL for the cover.

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::tools::{Tool, run_captured};

/// One chapter of the source video — a track, once it is cut out.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Chapter {
    pub title: String,
    pub start_time: f64,
    pub end_time: f64,
}

impl Chapter {
    pub fn duration(&self) -> f64 {
        self.end_time - self.start_time
    }
}

/// The subset of yt-dlp's JSON this tool actually reads.
#[derive(Debug, Clone, Deserialize)]
pub struct VideoInfo {
    pub title: String,
    #[serde(default)]
    pub uploader: Option<String>,
    /// `YYYYMMDD` — yt-dlp's format, not ISO. See [`VideoInfo::year`].
    #[serde(default)]
    pub upload_date: Option<String>,
    #[serde(default)]
    pub duration: Option<f64>,
    #[serde(default)]
    pub chapters: Option<Vec<Chapter>>,
}

impl VideoInfo {
    /// Chapters, or an empty slice. yt-dlp emits `null` (not `[]`) for a video
    /// with no chapter markers, which is why this is an `Option` underneath.
    pub fn chapters(&self) -> &[Chapter] {
        self.chapters.as_deref().unwrap_or(&[])
    }

    /// The upload year, parsed out of yt-dlp's `YYYYMMDD`.
    ///
    /// This is a fallback only: the upload date is when the video was posted,
    /// which for a concert recording is usually days after the performance.
    /// A `--year` override always wins.
    pub fn year(&self) -> Option<u16> {
        self.upload_date.as_ref()?.get(..4)?.parse().ok()
    }
}

/// Build the argv for the metadata probe.
pub fn probe_args(url: &str) -> Vec<String> {
    vec![
        "--dump-single-json".into(),
        "--no-warnings".into(),
        // Playlists would make `chapters` ambiguous (whose chapters?) and turn
        // one album into many. Refuse the ambiguity instead of guessing.
        "--no-playlist".into(),
        url.into(),
    ]
}

/// Fetch and parse the video's metadata.
pub fn probe(url: &str) -> Result<VideoInfo> {
    let json = run_captured(Tool::YtDlp, &probe_args(url))
        .with_context(|| format!("could not read video metadata for {url}"))?;
    parse_info(&json)
}

/// Parse yt-dlp's JSON. Split out from [`probe`] so it can be tested against
/// fixtures without a network.
pub fn parse_info(json: &str) -> Result<VideoInfo> {
    let info: VideoInfo =
        serde_json::from_str(json).context("yt-dlp returned JSON in an unexpected shape")?;

    if info.title.trim().is_empty() {
        bail!("video has no title");
    }
    Ok(info)
}

/// Reject chapter lists this tool cannot honestly turn into tracks.
///
/// A zero- or negative-length chapter would produce an empty file, and an
/// out-of-order list means the uploader's markers are not a track listing.
/// Both are better surfaced than silently written to disk.
pub fn validate_chapters(chapters: &[Chapter]) -> Result<()> {
    if chapters.is_empty() {
        bail!(
            "this video has no chapter markers — sleeve splits on the uploader's \
             chapters, so there is nothing to split on"
        );
    }

    for (i, ch) in chapters.iter().enumerate() {
        if ch.duration() <= 0.0 {
            bail!(
                "chapter {} ({:?}) has a non-positive duration ({:.1}s)",
                i + 1,
                ch.title,
                ch.duration()
            );
        }
        // Indexing rather than a let-chain: let-chains are unstable before
        // Rust 1.88 and this crate's MSRV is 1.85. `i > 0` already proves the
        // index is in range.
        if i > 0 && ch.start_time < chapters[i - 1].start_time {
            bail!(
                "chapter {} ({:?}) starts before the one before it",
                i + 1,
                ch.title
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "title": "Some Band - Live at the Hall [Full Show]",
        "uploader": "Some Uploader",
        "upload_date": "20260807",
        "duration": 300.0,
        "chapters": [
            {"title": "One", "start_time": 0.0, "end_time": 100.0},
            {"title": "Two", "start_time": 100.0, "end_time": 300.0}
        ]
    }"#;

    #[test]
    fn parses_the_fields_the_pipeline_uses() {
        let info = parse_info(SAMPLE).unwrap();
        assert_eq!(info.title, "Some Band - Live at the Hall [Full Show]");
        assert_eq!(info.uploader.as_deref(), Some("Some Uploader"));
        assert_eq!(info.chapters().len(), 2);
        assert_eq!(info.chapters()[1].duration(), 200.0);
    }

    #[test]
    fn year_comes_from_the_first_four_digits_of_upload_date() {
        assert_eq!(parse_info(SAMPLE).unwrap().year(), Some(2026));
    }

    #[test]
    fn missing_upload_date_yields_no_year() {
        let json = r#"{"title": "T"}"#;
        assert_eq!(parse_info(json).unwrap().year(), None);
    }

    #[test]
    fn null_chapters_read_as_empty_not_as_a_parse_error() {
        // yt-dlp emits `null`, not `[]`, for an unchaptered video.
        let info = parse_info(r#"{"title": "T", "chapters": null}"#).unwrap();
        assert!(info.chapters().is_empty());
    }

    #[test]
    fn unchaptered_video_is_rejected_with_an_explanation() {
        let err = validate_chapters(&[]).unwrap_err().to_string();
        assert!(err.contains("no chapter markers"), "{err}");
    }

    #[test]
    fn zero_length_chapter_is_rejected() {
        let chapters = vec![Chapter {
            title: "Empty".into(),
            start_time: 10.0,
            end_time: 10.0,
        }];
        let err = validate_chapters(&chapters).unwrap_err().to_string();
        assert!(err.contains("non-positive"), "{err}");
    }

    #[test]
    fn out_of_order_chapters_are_rejected() {
        let chapters = vec![
            Chapter {
                title: "A".into(),
                start_time: 100.0,
                end_time: 200.0,
            },
            Chapter {
                title: "B".into(),
                start_time: 50.0,
                end_time: 150.0,
            },
        ];
        assert!(validate_chapters(&chapters).is_err());
    }

    #[test]
    fn well_formed_chapters_pass() {
        let info = parse_info(SAMPLE).unwrap();
        assert!(validate_chapters(info.chapters()).is_ok());
    }

    #[test]
    fn probe_args_refuse_playlists() {
        let args = probe_args("https://youtu.be/abc");
        assert!(args.contains(&"--no-playlist".to_string()));
        assert_eq!(args.last().unwrap(), "https://youtu.be/abc");
    }
}
