//! The command-line surface.

use clap::Parser;
use std::path::PathBuf;

use crate::config::Format;

/// Split a chapter-marked YouTube video into tagged audio tracks.
#[derive(Debug, Parser)]
#[command(name = "sleeve", version, about, long_about = None)]
pub struct Cli {
    /// Video URL. Must have chapter markers — those are the track boundaries.
    pub url: String,

    /// Where to write the album folder [default: your Desktop]
    #[arg(short, long, env = "SLEEVE_DEST")]
    pub dest: Option<PathBuf>,

    /// Audio format [default: m4a]
    #[arg(short, long, env = "SLEEVE_FORMAT", value_enum)]
    pub format: Option<Format>,

    /// Audio bitrate, e.g. 256k [default: per-format]
    #[arg(short, long, env = "SLEEVE_BITRATE")]
    pub bitrate: Option<String>,

    /// Album artist [default: inferred from the video title]
    #[arg(long)]
    pub artist: Option<String>,

    /// Album title [default: inferred from the video title]
    #[arg(long)]
    pub album: Option<String>,

    /// Release year [default: the video's upload year]
    #[arg(long)]
    pub year: Option<u16>,

    /// Genre tag
    #[arg(long, env = "SLEEVE_GENRE")]
    pub genre: Option<String>,

    /// Comment tag written to every track
    #[arg(long)]
    pub comment: Option<String>,

    /// Also add the finished tracks to Apple Music (macOS)
    #[arg(short, long)]
    pub music: bool,

    /// Keep the un-split full-length audio alongside the tracks
    #[arg(long)]
    pub keep_full: bool,

    /// Skip cover art entirely
    #[arg(long)]
    pub no_cover: bool,

    /// Write tracks straight into the destination instead of an album subfolder
    #[arg(long)]
    pub flat: bool,

    /// Print the track listing and what would be written, then stop
    #[arg(short = 'n', long)]
    pub dry_run: bool,

    /// Print the chapter list and exit — no download
    #[arg(short, long)]
    pub list: bool,
}

impl Cli {
    /// The album folder's name, or `None` when `--flat` was given.
    pub fn album_dir_name(&self, album: &str, artist: Option<&str>) -> Option<String> {
        if self.flat {
            return None;
        }
        Some(match artist {
            Some(a) => crate::naming::safe_stem(&format!("{a} - {album}")),
            None => crate::naming::safe_stem(album),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn parse(args: &[&str]) -> Cli {
        Cli::parse_from(std::iter::once("sleeve").chain(args.iter().copied()))
    }

    #[test]
    fn the_command_definition_is_valid() {
        // Catches duplicate short flags and malformed value_enums at test time
        // rather than on the user's first run.
        Cli::command().debug_assert();
    }

    #[test]
    fn url_is_the_only_required_argument() {
        let cli = parse(&["https://youtu.be/abc"]);
        assert_eq!(cli.url, "https://youtu.be/abc");
        assert!(cli.dest.is_none());
        assert!(!cli.music);
    }

    #[test]
    fn flags_parse_to_the_expected_shape() {
        let cli = parse(&[
            "https://youtu.be/abc",
            "--dest",
            "/tmp/out",
            "--format",
            "mp3",
            "--artist",
            "The Band",
            "--year",
            "1974",
            "--music",
        ]);
        assert_eq!(cli.dest, Some(PathBuf::from("/tmp/out")));
        assert_eq!(cli.format, Some(Format::Mp3));
        assert_eq!(cli.artist.as_deref(), Some("The Band"));
        assert_eq!(cli.year, Some(1974));
        assert!(cli.music);
    }

    #[test]
    fn album_folder_is_artist_and_album() {
        let cli = parse(&["u"]);
        assert_eq!(
            cli.album_dir_name("The Album", Some("The Band")),
            Some("The Band - The Album".to_string())
        );
    }

    #[test]
    fn album_folder_omits_a_missing_artist() {
        let cli = parse(&["u"]);
        assert_eq!(
            cli.album_dir_name("Just An Album", None),
            Some("Just An Album".into())
        );
    }

    #[test]
    fn album_folder_is_sanitised_like_any_other_path_component() {
        let cli = parse(&["u"]);
        let name = cli.album_dir_name("Live 8/7/26", Some("A/B")).unwrap();
        assert!(!name.contains('/'), "{name}");
    }

    #[test]
    fn flat_suppresses_the_album_folder() {
        let cli = parse(&["u", "--flat"]);
        assert_eq!(cli.album_dir_name("The Album", Some("The Band")), None);
    }

    #[test]
    fn an_invalid_format_is_rejected_at_parse_time() {
        let err = Cli::try_parse_from(["sleeve", "u", "--format", "flac"]);
        assert!(err.is_err());
    }
}
