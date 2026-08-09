//! Turning a video title into filenames and tags.
//!
//! Everything here is a guess with an override. The inference exists so the
//! common case (`Artist - Album [Full Album]`) needs no flags at all; the
//! overrides exist because the guess is wrong often enough that a tool which
//! could not be corrected would be useless.

/// Characters that cannot appear in a path component on the platforms this
/// targets. `/` is the POSIX separator and `:` is the classic-Mac separator
/// that Finder still displays as `/`.
const UNSAFE_IN_FILENAME: &[char] = &['/', '\\', ':'];

/// A filename long enough to survive HFS+/APFS (255 bytes) with room for the
/// numeric prefix and extension. Multi-byte titles make this a byte budget,
/// not a character count.
const MAX_STEM_BYTES: usize = 180;

/// Bracketed noise that uploaders append to titles. Matched case-insensitively
/// against the *contents* of a trailing `[...]` or `(...)` group.
const NOISE_TAGS: &[&str] = &[
    "full album",
    "full show",
    "full concert",
    "full set",
    "official video",
    "official audio",
    "official music video",
    "hd",
    "hq",
    "4k",
    "1080p",
    "720p",
    "remastered",
    "audio only",
    "lyrics",
    "visualizer",
];

/// Make a chapter title safe to use as a filename stem.
///
/// Returns `None`-ish behaviour via a placeholder rather than failing: a
/// chapter titled `"/"` is odd but not a reason to abandon a 23-track rip.
pub fn safe_stem(title: &str) -> String {
    let mut s: String = title
        .chars()
        .map(|c| {
            // Whitespace is normalised, not replaced: a tab in a title means a
            // space, and turning it into `-` would read as punctuation the
            // uploader never wrote. Other control characters have no such
            // reading and become `-` like any unsafe character.
            if c.is_whitespace() {
                ' '
            } else if UNSAFE_IN_FILENAME.contains(&c) || c.is_control() {
                '-'
            } else {
                c
            }
        })
        .collect();

    s = collapse_whitespace(&s);
    // A leading dot hides the file on Unix; a trailing dot or space is
    // silently dropped by some filesystems, which breaks later lookups.
    s = s
        .trim_matches(|c: char| c == '.' || c.is_whitespace())
        .to_string();
    truncate_bytes(&mut s, MAX_STEM_BYTES);

    if s.is_empty() { "untitled".into() } else { s }
}

/// `"01"`, `"07"`, `"12"` — width follows the total so a 100-track rip still
/// sorts lexicographically.
pub fn track_prefix(index: usize, total: usize) -> String {
    let width = total.to_string().len().max(2);
    format!("{index:0width$}")
}

/// The full filename for a track, e.g. `"03 - Slow Ascent.m4a"`.
pub fn track_filename(index: usize, total: usize, title: &str, extension: &str) -> String {
    format!(
        "{} - {}.{}",
        track_prefix(index, total),
        safe_stem(title),
        extension
    )
}

/// Artist and album guessed from a video title.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InferredTags {
    pub artist: Option<String>,
    pub album: String,
}

/// Split `Artist - Album` out of a video title, dropping uploader noise.
///
/// Splits on the **first** ` - `, because album titles contain dashes far more
/// often than artist names do.
pub fn infer_tags(video_title: &str) -> InferredTags {
    let cleaned = strip_noise_tags(video_title);

    match cleaned.split_once(" - ") {
        Some((artist, album)) => {
            let artist = artist.trim();
            let album = album.trim();
            // A split that leaves either side empty is not a real split.
            if artist.is_empty() || album.is_empty() {
                InferredTags {
                    artist: None,
                    album: cleaned.clone(),
                }
            } else {
                InferredTags {
                    artist: Some(artist.to_string()),
                    album: album.to_string(),
                }
            }
        }
        None => InferredTags {
            artist: None,
            album: cleaned,
        },
    }
}

/// Remove trailing `[...]`/`(...)` groups whose contents are known noise.
///
/// Only *known* noise is removed. `(Live)` and `(Deluxe Edition)` are part of
/// an album's identity, so an allowlist beats a blanket "strip all brackets".
pub fn strip_noise_tags(title: &str) -> String {
    let mut s = title.trim().to_string();

    // Uploaders stack these — `[Full Show] (HD) [4K]` is one title, three tags.
    while let Some(trimmed) = strip_one_trailing_group(&s) {
        s = trimmed;
    }
    collapse_whitespace(&s).trim().to_string()
}

fn strip_one_trailing_group(s: &str) -> Option<String> {
    let trimmed = s.trim_end();
    let (open, close) = match trimmed.chars().last()? {
        ']' => ('[', ']'),
        ')' => ('(', ')'),
        _ => return None,
    };

    let open_idx = trimmed.rfind(open)?;
    let inner = &trimmed[open_idx + open.len_utf8()..trimmed.len() - close.len_utf8()];
    let inner_lower = inner.trim().to_lowercase();

    if NOISE_TAGS.contains(&inner_lower.as_str()) {
        Some(trimmed[..open_idx].to_string())
    } else {
        None
    }
}

fn collapse_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_space = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !in_space {
                out.push(' ');
            }
            in_space = true;
        } else {
            out.push(c);
            in_space = false;
        }
    }
    out
}

/// Truncate to a byte budget without splitting a UTF-8 character.
fn truncate_bytes(s: &mut String, max: usize) {
    if s.len() <= max {
        return;
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s.truncate(end);
    let trimmed = s.trim_end().to_string();
    *s = trimmed;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_separators_become_dashes() {
        // The real case this came from: a chapter titled with a date.
        assert_eq!(safe_stem("Live 8/7/26"), "Live 8-7-26");
        assert_eq!(safe_stem(r"A\B"), "A-B");
        assert_eq!(safe_stem("A:B"), "A-B");
    }

    #[test]
    fn apostrophes_and_ampersands_survive() {
        // These are legal in filenames and stripping them mangles real titles.
        assert_eq!(
            safe_stem("Heron's Landing Part II"),
            "Heron's Landing Part II"
        );
        assert_eq!(
            safe_stem("Nightjar, Wren & Friends"),
            "Nightjar, Wren & Friends"
        );
    }

    #[test]
    fn whitespace_is_collapsed_and_trimmed() {
        assert_eq!(safe_stem("  A   B  "), "A B");
        assert_eq!(safe_stem("A\tB"), "A B");
    }

    #[test]
    fn leading_dots_are_stripped_so_tracks_are_not_hidden() {
        assert_eq!(safe_stem(".hidden"), "hidden");
        assert_eq!(safe_stem("trailing."), "trailing");
    }

    #[test]
    fn a_title_of_only_unsafe_characters_still_yields_a_filename() {
        assert_eq!(safe_stem("..."), "untitled");
        assert_eq!(safe_stem("   "), "untitled");
    }

    #[test]
    fn long_titles_truncate_on_a_char_boundary() {
        let long = "é".repeat(200); // 400 bytes
        let stem = safe_stem(&long);
        assert!(stem.len() <= MAX_STEM_BYTES);
        assert!(stem.chars().all(|c| c == 'é'), "truncated mid-character");
    }

    #[test]
    fn track_prefix_widens_with_the_total() {
        assert_eq!(track_prefix(1, 9), "01"); // never narrower than 2
        assert_eq!(track_prefix(7, 23), "07");
        assert_eq!(track_prefix(7, 100), "007");
    }

    #[test]
    fn track_filename_composes_prefix_stem_and_extension() {
        assert_eq!(
            track_filename(3, 23, "Slow Ascent", "m4a"),
            "03 - Slow Ascent.m4a"
        );
    }

    #[test]
    fn artist_and_album_split_on_the_first_dash() {
        let tags = infer_tags("Some Band - Live - Second Night");
        assert_eq!(tags.artist.as_deref(), Some("Some Band"));
        assert_eq!(tags.album, "Live - Second Night");
    }

    #[test]
    fn trailing_noise_tags_are_dropped() {
        let tags = infer_tags("Some Band - The Album [Full Album]");
        assert_eq!(tags.artist.as_deref(), Some("Some Band"));
        assert_eq!(tags.album, "The Album");
    }

    #[test]
    fn several_stacked_noise_tags_are_all_dropped() {
        assert_eq!(strip_noise_tags("A Show [Full Show] (HD) [4K]"), "A Show");
    }

    #[test]
    fn meaningful_parentheses_are_kept() {
        // (Live) and (Deluxe Edition) are part of the album's name.
        assert_eq!(strip_noise_tags("The Album (Live)"), "The Album (Live)");
        assert_eq!(
            strip_noise_tags("The Album (Deluxe Edition)"),
            "The Album (Deluxe Edition)"
        );
    }

    #[test]
    fn a_title_without_a_dash_becomes_an_album_with_no_artist() {
        let tags = infer_tags("Just A Concert Recording");
        assert_eq!(tags.artist, None);
        assert_eq!(tags.album, "Just A Concert Recording");
    }

    #[test]
    fn a_dangling_dash_is_not_treated_as_a_split() {
        let tags = infer_tags(" - Album");
        assert_eq!(tags.artist, None);
    }

    #[test]
    fn noise_matching_is_case_insensitive() {
        assert_eq!(strip_noise_tags("X [FULL ALBUM]"), "X");
        assert_eq!(strip_noise_tags("X [Full album]"), "X");
    }
}
