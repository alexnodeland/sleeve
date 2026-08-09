//! Output format, and where settings come from.
//!
//! Precedence, highest first: command-line flag, `SLEEVE_*` environment
//! variable (both handled by clap), the config file, then the built-in
//! default. The config file exists so the settings you never vary — your
//! destination, your preferred format — do not have to be retyped.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Audio container and codec for the emitted tracks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
#[clap(rename_all = "lowercase")]
pub enum Format {
    /// AAC in MP4. The default: Apple-native, embeds cover art, widely played.
    #[default]
    M4a,
    /// MP3. Larger at equal quality, but plays on everything ever made.
    Mp3,
    /// Opus in Ogg. Stream-copied from YouTube's own audio, so it is the only
    /// lossless-relative-to-source option — at the cost of no embedded cover
    /// and no Apple Music support.
    Opus,
}

impl Format {
    pub const fn extension(self) -> &'static str {
        match self {
            Format::M4a => "m4a",
            Format::Mp3 => "mp3",
            Format::Opus => "opus",
        }
    }

    /// ffmpeg `-c:a` value, or `None` when the stream is copied verbatim.
    ///
    /// Opus copies because YouTube's best audio *is* Opus: re-encoding it
    /// would spend quality to produce a larger file.
    pub const fn codec(self) -> Option<&'static str> {
        match self {
            Format::M4a => Some("aac"),
            Format::Mp3 => Some("libmp3lame"),
            Format::Opus => None,
        }
    }

    /// Whether cover art can be embedded in this container by ffmpeg.
    ///
    /// Ogg/Opus carries art as a base64 `METADATA_BLOCK_PICTURE` comment,
    /// which ffmpeg does not write. Rather than silently drop the art, the
    /// pipeline writes `cover.jpg` beside the tracks for these formats.
    pub const fn supports_embedded_cover(self) -> bool {
        match self {
            Format::M4a | Format::Mp3 => true,
            Format::Opus => false,
        }
    }

    /// Whether Apple Music will import this format.
    pub const fn importable_by_music_app(self) -> bool {
        match self {
            Format::M4a | Format::Mp3 => true,
            Format::Opus => false,
        }
    }

    pub const fn default_bitrate(self) -> &'static str {
        match self {
            Format::M4a => "256k",
            // Below 320k, MP3's quality deficit versus AAC becomes audible on
            // cymbals and applause — exactly what a live recording is full of.
            Format::Mp3 => "320k",
            Format::Opus => "copy",
        }
    }
}

/// The on-disk config file. Every field is optional; absent means "fall back".
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct FileConfig {
    pub dest: Option<PathBuf>,
    pub format: Option<Format>,
    pub bitrate: Option<String>,
    pub genre: Option<String>,
    /// Add finished tracks to Apple Music without needing `--music` each run.
    pub add_to_music: Option<bool>,
    /// Keep the un-split full-length audio alongside the tracks.
    pub keep_full: Option<bool>,
}

impl FileConfig {
    /// Parse a config file's contents.
    pub fn parse(toml_str: &str) -> Result<Self> {
        toml::from_str(toml_str).context("config file is not valid TOML for sleeve")
    }

    /// Load from `path`, treating a missing file as an empty config.
    ///
    /// Not having a config file is the normal state, not an error.
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(s) => Self::parse(&s).with_context(|| format!("in {}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).with_context(|| format!("could not read {}", path.display())),
        }
    }
}

/// Where the config file lives: `~/.config/sleeve/config.toml` on macOS and
/// Linux alike.
///
/// `directories` would put this under `~/Library/Application Support` on
/// macOS, which is correct for app data but wrong for a CLI's dotfile — no
/// one hand-edits TOML in there.
pub fn config_path() -> Option<PathBuf> {
    let home = directories::BaseDirs::new()?.home_dir().to_path_buf();
    Some(home.join(".config").join("sleeve").join("config.toml"))
}

/// The default output directory: the user's Desktop, or `$HOME` if there is
/// no Desktop (a headless Linux box, say).
pub fn default_dest() -> PathBuf {
    if let Some(dirs) = directories::UserDirs::new() {
        if let Some(desktop) = dirs.desktop_dir() {
            return desktop.to_path_buf();
        }
        return dirs.home_dir().to_path_buf();
    }
    PathBuf::from(".")
}

/// Fully resolved settings for one run.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub dest: PathBuf,
    pub format: Format,
    pub bitrate: String,
    pub genre: Option<String>,
    pub add_to_music: bool,
    pub keep_full: bool,
}

impl Config {
    /// Apply the precedence rules. `cli_*` values are `None` when the flag was
    /// not given, which is what lets the file win over the built-in default
    /// without overriding an explicit flag.
    pub fn resolve(
        file: &FileConfig,
        cli_dest: Option<PathBuf>,
        cli_format: Option<Format>,
        cli_bitrate: Option<String>,
        cli_genre: Option<String>,
        cli_music: bool,
        cli_keep_full: bool,
    ) -> Self {
        let format = cli_format.or(file.format).unwrap_or_default();
        Self {
            dest: cli_dest
                .or_else(|| file.dest.clone())
                .unwrap_or_else(default_dest),
            format,
            bitrate: cli_bitrate
                .or_else(|| file.bitrate.clone())
                .unwrap_or_else(|| format.default_bitrate().to_string()),
            genre: cli_genre.or_else(|| file.genre.clone()),
            // Booleans are flags: present means true, and the file can turn
            // them on by default. There is deliberately no `--no-music`; the
            // file is the place to express a standing preference.
            add_to_music: cli_music || file.add_to_music.unwrap_or(false),
            keep_full: cli_keep_full || file.keep_full.unwrap_or(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_extensions_and_codecs_line_up() {
        assert_eq!(Format::M4a.extension(), "m4a");
        assert_eq!(Format::M4a.codec(), Some("aac"));
        assert_eq!(Format::Mp3.codec(), Some("libmp3lame"));
        // Opus is copied, never encoded.
        assert_eq!(Format::Opus.codec(), None);
    }

    #[test]
    fn only_the_copied_format_lacks_cover_and_music_support() {
        for f in [Format::M4a, Format::Mp3] {
            assert!(f.supports_embedded_cover(), "{f:?}");
            assert!(f.importable_by_music_app(), "{f:?}");
        }
        assert!(!Format::Opus.supports_embedded_cover());
        assert!(!Format::Opus.importable_by_music_app());
    }

    #[test]
    fn empty_config_file_parses_to_all_defaults() {
        assert_eq!(FileConfig::parse("").unwrap(), FileConfig::default());
    }

    #[test]
    fn config_file_uses_kebab_case_keys() {
        let cfg = FileConfig::parse(
            r#"
            dest = "/tmp/music"
            format = "mp3"
            add-to-music = true
            keep-full = true
            "#,
        )
        .unwrap();
        assert_eq!(cfg.dest, Some(PathBuf::from("/tmp/music")));
        assert_eq!(cfg.format, Some(Format::Mp3));
        assert_eq!(cfg.add_to_music, Some(true));
        assert_eq!(cfg.keep_full, Some(true));
    }

    #[test]
    fn unknown_config_keys_are_an_error_not_a_silent_no_op() {
        // A typo'd key that is ignored looks exactly like a setting that does
        // not work, which is the worst possible failure mode for a config file.
        let err = FileConfig::parse("destination = \"/tmp\"").unwrap_err();
        assert!(err.to_string().contains("not valid TOML for sleeve"));
    }

    #[test]
    fn missing_config_file_is_not_an_error() {
        let cfg = FileConfig::load(Path::new("/nonexistent/sleeve/config.toml")).unwrap();
        assert_eq!(cfg, FileConfig::default());
    }

    #[test]
    fn cli_beats_file_beats_default() {
        let file = FileConfig {
            dest: Some("/from/file".into()),
            format: Some(Format::Mp3),
            ..Default::default()
        };

        let from_file = Config::resolve(&file, None, None, None, None, false, false);
        assert_eq!(from_file.dest, PathBuf::from("/from/file"));
        assert_eq!(from_file.format, Format::Mp3);

        let from_cli = Config::resolve(
            &file,
            Some("/from/cli".into()),
            Some(Format::Opus),
            None,
            None,
            false,
            false,
        );
        assert_eq!(from_cli.dest, PathBuf::from("/from/cli"));
        assert_eq!(from_cli.format, Format::Opus);
    }

    #[test]
    fn bitrate_defaults_track_the_resolved_format() {
        // The default must follow the format that actually won the precedence
        // fight, not the built-in default format.
        let file = FileConfig {
            format: Some(Format::Mp3),
            ..Default::default()
        };
        let cfg = Config::resolve(&file, None, None, None, None, false, false);
        assert_eq!(cfg.bitrate, "320k");

        let cfg = Config::resolve(&FileConfig::default(), None, None, None, None, false, false);
        assert_eq!(cfg.bitrate, "256k");
    }

    #[test]
    fn the_music_flag_ors_with_the_file_setting() {
        let file = FileConfig {
            add_to_music: Some(true),
            ..Default::default()
        };
        assert!(Config::resolve(&file, None, None, None, None, false, false).add_to_music);

        let file = FileConfig::default();
        assert!(Config::resolve(&file, None, None, None, None, true, false).add_to_music);
        assert!(!Config::resolve(&file, None, None, None, None, false, false).add_to_music);
    }

    #[test]
    fn config_path_is_a_hand_editable_dotfile() {
        // Not ~/Library/Application Support — see the doc comment.
        if let Some(p) = config_path() {
            let s = p.to_string_lossy();
            assert!(s.contains("/.config/sleeve/config.toml"), "{s}");
        }
    }
}
