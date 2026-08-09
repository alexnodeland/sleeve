//! Cutting the downloaded audio into tagged tracks.
//!
//! Three decisions here are the difference between output that looks right and
//! output that *is* right, and each one came from getting it wrong first:
//!
//! * **Cut from the source download, not from an intermediate.** Every track
//!   is then one encode away from YouTube's audio instead of two.
//! * **`-map_chapters -1` on every track.** A container that inherits the
//!   source's chapter list advertises all 23 songs inside track 3, and players
//!   present it as the whole album. The durations look fine; the file lies.
//! * **`-map_metadata -1` before setting tags.** Otherwise the source's global
//!   title and comment ride along underneath the tags actually wanted.
//!
//! Audio cuts are sample-accurate because the stream is re-encoded, so unlike
//! a stream-copied video split there is no keyframe drift at the boundaries.

use std::path::{Path, PathBuf};

use crate::config::Format;
use crate::naming::track_filename;
use crate::probe::Chapter;

/// Everything written into one track's tags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackTags {
    pub title: String,
    pub artist: Option<String>,
    pub album: String,
    pub album_artist: Option<String>,
    pub genre: Option<String>,
    pub year: Option<u16>,
    pub comment: Option<String>,
    pub track: usize,
    pub total: usize,
}

/// yt-dlp args to pull the best audio stream, unmodified.
///
/// Deliberately not `-x/--audio-format`: that would transcode on download, and
/// every track cut from it would inherit that generation loss on top of its
/// own. The container is whatever YouTube served — the caller finds it by
/// glob, since the extension is not known ahead of time.
pub fn download_args(url: &str, out_template: &str) -> Vec<String> {
    vec![
        "-f".into(),
        "bestaudio".into(),
        "--no-playlist".into(),
        "--no-warnings".into(),
        "-o".into(),
        out_template.into(),
        url.into(),
    ]
}

/// Resolve `-c:a` for the target format given what the source actually is.
///
/// Opus output copies when the source is already Opus — the usual case, and
/// the only way to emit audio that is bit-identical to YouTube's. When the
/// source is something else, it has to be encoded, so the "lossless" promise
/// quietly does not apply and libopus does the work.
pub fn codec_args(format: Format, source_codec: Option<&str>, bitrate: &str) -> Vec<String> {
    match format.codec() {
        Some(codec) => vec!["-c:a".into(), codec.into(), "-b:a".into(), bitrate.into()],
        None => {
            if source_codec == Some("opus") {
                vec!["-c:a".into(), "copy".into()]
            } else {
                // Not the source codec, so this is a real encode. 160k is
                // Opus's transparent-for-music point and matches what YouTube
                // itself serves.
                vec![
                    "-c:a".into(),
                    "libopus".into(),
                    "-b:a".into(),
                    "160k".into(),
                ]
            }
        }
    }
}

/// Metadata args, in a fixed order so the tests can assert on them.
pub fn metadata_args(tags: &TrackTags) -> Vec<String> {
    let mut args = Vec::new();
    let mut push = |k: &str, v: &str| {
        args.push("-metadata".to_string());
        args.push(format!("{k}={v}"));
    };

    push("title", &tags.title);
    if let Some(a) = &tags.artist {
        push("artist", a);
    }
    push("album", &tags.album);
    // Without album_artist, a player files each track under its own artist and
    // the album fragments — the classic compilation-album failure.
    if let Some(a) = tags.album_artist.as_ref().or(tags.artist.as_ref()) {
        push("album_artist", a);
    }
    if let Some(g) = &tags.genre {
        push("genre", g);
    }
    if let Some(y) = tags.year {
        push("date", &y.to_string());
    }
    if let Some(c) = &tags.comment {
        push("comment", c);
    }
    push("track", &format!("{}/{}", tags.track, tags.total));
    push("disc", "1/1");
    args
}

/// The complete ffmpeg argv for one track.
// Every parameter here is an independent axis of the ffmpeg invocation, and
// this function is called from exactly one place. Bundling them into a struct
// would add a type whose only job is to be destructured immediately, and would
// make the arg-order tests below read through an extra indirection.
#[allow(clippy::too_many_arguments)]
pub fn encode_args(
    source: &Path,
    cover: Option<&Path>,
    chapter: &Chapter,
    tags: &TrackTags,
    format: Format,
    source_codec: Option<&str>,
    bitrate: &str,
    output: &Path,
) -> Vec<String> {
    let mut args: Vec<String> = vec!["-v".into(), "error".into(), "-y".into()];

    // Input seeking (-ss before -i) so ffmpeg jumps rather than decoding the
    // whole file up to the cut point. On a two-hour source that is the
    // difference between a second and a minute per track.
    args.push("-ss".into());
    args.push(format!("{:.3}", chapter.start_time));
    args.push("-to".into());
    args.push(format!("{:.3}", chapter.end_time));
    args.push("-i".into());
    args.push(source.to_string_lossy().into_owned());

    let embed_cover = cover.is_some() && format.supports_embedded_cover();
    if embed_cover {
        args.push("-i".into());
        args.push(cover.unwrap().to_string_lossy().into_owned());
    }

    args.push("-map".into());
    args.push("0:a".into());
    if embed_cover {
        args.push("-map".into());
        args.push("1:v".into());
    }

    // Start from no metadata and no chapters, then add exactly what is wanted.
    args.push("-map_metadata".into());
    args.push("-1".into());
    args.push("-map_chapters".into());
    args.push("-1".into());

    args.extend(codec_args(format, source_codec, bitrate));

    if embed_cover {
        args.push("-c:v".into());
        args.push("mjpeg".into());
        args.push("-disposition:v".into());
        args.push("attached_pic".into());
    }

    args.extend(metadata_args(tags));

    // Move the index to the front so players can start without reading the
    // whole file. MP4-family only; harmless to omit elsewhere.
    if matches!(format, Format::M4a) {
        args.push("-movflags".into());
        args.push("+faststart".into());
    }

    args.push(output.to_string_lossy().into_owned());
    args
}

/// Where a given track will be written.
pub fn output_path(dir: &Path, index: usize, total: usize, title: &str, format: Format) -> PathBuf {
    dir.join(track_filename(index, total, title, format.extension()))
}

/// Tags for track `index`, combining per-chapter and per-album facts.
#[allow(clippy::too_many_arguments)]
pub fn tags_for(
    chapter: &Chapter,
    index: usize,
    total: usize,
    artist: Option<&str>,
    album: &str,
    genre: Option<&str>,
    year: Option<u16>,
    comment: Option<&str>,
) -> TrackTags {
    TrackTags {
        title: chapter.title.clone(),
        artist: artist.map(str::to_string),
        album: album.to_string(),
        album_artist: artist.map(str::to_string),
        genre: genre.map(str::to_string),
        year,
        comment: comment.map(str::to_string),
        track: index,
        total,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chapter() -> Chapter {
        Chapter {
            title: "Marsh Light".into(),
            start_time: 5528.0,
            end_time: 5948.0,
        }
    }

    fn tags() -> TrackTags {
        TrackTags {
            title: "Marsh Light".into(),
            artist: Some("The Band".into()),
            album: "Live Somewhere".into(),
            album_artist: Some("The Band".into()),
            genre: Some("Progressive Rock".into()),
            year: Some(2026),
            comment: Some("Recorded live.".into()),
            track: 17,
            total: 23,
        }
    }

    fn args_for_default_format() -> Vec<String> {
        encode_args(
            Path::new("src.webm"),
            Some(Path::new("cover.jpg")),
            &chapter(),
            &tags(),
            Format::M4a,
            Some("opus"),
            "256k",
            Path::new("17 - Marsh Light.m4a"),
        )
    }

    fn pair_present(args: &[String], a: &str, b: &str) -> bool {
        args.windows(2).any(|w| w[0] == a && w[1] == b)
    }

    #[test]
    fn every_track_drops_the_sources_chapter_list() {
        // The bug this guards: without it, each track advertises the whole
        // album's chapters and players present it as the full show.
        assert!(pair_present(
            &args_for_default_format(),
            "-map_chapters",
            "-1"
        ));
    }

    #[test]
    fn every_track_starts_from_clean_metadata() {
        assert!(pair_present(
            &args_for_default_format(),
            "-map_metadata",
            "-1"
        ));
    }

    #[test]
    fn seek_precedes_input_for_fast_seeking() {
        let args = args_for_default_format();
        let ss = args.iter().position(|a| a == "-ss").unwrap();
        let i = args.iter().position(|a| a == "-i").unwrap();
        assert!(ss < i, "-ss must come before -i to seek rather than decode");
    }

    #[test]
    fn cut_points_come_from_the_chapter_verbatim() {
        let args = args_for_default_format();
        assert!(pair_present(&args, "-ss", "5528.000"));
        assert!(pair_present(&args, "-to", "5948.000"));
    }

    #[test]
    fn cover_is_mapped_as_an_attached_picture() {
        let args = args_for_default_format();
        assert!(pair_present(&args, "-map", "1:v"));
        assert!(pair_present(&args, "-disposition:v", "attached_pic"));
        assert!(pair_present(&args, "-c:v", "mjpeg"));
    }

    #[test]
    fn opus_output_takes_no_cover_input_at_all() {
        // Ogg cover art is a base64 comment ffmpeg will not write; mapping a
        // video stream into an .opus file just fails.
        let args = encode_args(
            Path::new("src.webm"),
            Some(Path::new("cover.jpg")),
            &chapter(),
            &tags(),
            Format::Opus,
            Some("opus"),
            "copy",
            Path::new("17 - Marsh Light.opus"),
        );
        assert!(!pair_present(&args, "-map", "1:v"));
        assert!(!args.iter().any(|a| a == "cover.jpg"));
    }

    #[test]
    fn opus_from_an_opus_source_is_copied_not_re_encoded() {
        let args = codec_args(Format::Opus, Some("opus"), "ignored");
        assert_eq!(args, vec!["-c:a", "copy"]);
    }

    #[test]
    fn opus_from_a_non_opus_source_falls_back_to_encoding() {
        let args = codec_args(Format::Opus, Some("aac"), "ignored");
        assert!(pair_present(&args, "-c:a", "libopus"));
    }

    #[test]
    fn lossy_formats_always_encode_at_the_requested_bitrate() {
        assert!(pair_present(
            &codec_args(Format::M4a, Some("opus"), "256k"),
            "-b:a",
            "256k"
        ));
        assert!(pair_present(
            &codec_args(Format::Mp3, Some("opus"), "320k"),
            "-c:a",
            "libmp3lame"
        ));
    }

    #[test]
    fn faststart_is_mp4_only() {
        assert!(args_for_default_format().contains(&"+faststart".to_string()));

        let mp3 = encode_args(
            Path::new("s.webm"),
            None,
            &chapter(),
            &tags(),
            Format::Mp3,
            Some("opus"),
            "320k",
            Path::new("o.mp3"),
        );
        assert!(!mp3.contains(&"+faststart".to_string()));
    }

    #[test]
    fn track_and_disc_are_written_as_position_of_total() {
        let args = metadata_args(&tags());
        assert!(args.contains(&"track=17/23".to_string()));
        assert!(args.contains(&"disc=1/1".to_string()));
    }

    #[test]
    fn album_artist_defaults_to_the_artist_so_albums_do_not_fragment() {
        let mut t = tags();
        t.album_artist = None;
        assert!(metadata_args(&t).contains(&"album_artist=The Band".to_string()));
    }

    #[test]
    fn absent_optional_tags_are_omitted_entirely() {
        let t = TrackTags {
            title: "X".into(),
            artist: None,
            album: "A".into(),
            album_artist: None,
            genre: None,
            year: None,
            comment: None,
            track: 1,
            total: 1,
        };
        let args = metadata_args(&t);
        assert!(!args.iter().any(|a| a.starts_with("artist=")));
        assert!(!args.iter().any(|a| a.starts_with("genre=")));
        assert!(!args.iter().any(|a| a.starts_with("date=")));
        // ...but the required ones are still there.
        assert!(args.contains(&"title=X".to_string()));
        assert!(args.contains(&"album=A".to_string()));
    }

    #[test]
    fn output_path_uses_the_formats_extension() {
        let p = output_path(Path::new("/out"), 3, 23, "Slow Ascent", Format::Mp3);
        assert_eq!(p, Path::new("/out/03 - Slow Ascent.mp3"));
    }

    #[test]
    fn download_does_not_transcode() {
        let args = download_args("https://youtu.be/x", "src.%(ext)s");
        assert!(pair_present(&args, "-f", "bestaudio"));
        // -x / --audio-format would re-encode before we ever cut.
        assert!(!args.iter().any(|a| a == "-x" || a == "--audio-format"));
    }
}
