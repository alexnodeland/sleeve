//! Handing finished tracks to Apple Music.
//!
//! Not via AppleScript. Scripting Music needs the Automation entitlement, and
//! when the user has denied it — or has simply never been asked, in a context
//! with no one at the keyboard to click the prompt — the script hangs rather
//! than failing. macOS's own watched-folder mechanism needs no entitlement:
//! anything dropped into it is imported by Music on its next scan.
//!
//! Files are **copied** in, not moved, because Music consumes what it finds
//! there. Moving would make the output directory empty on success, which is a
//! surprising way to lose the tracks you just made.

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};

/// Directory names Apple has used for the watched import folder.
///
/// `.localized` suffixes are real directory names on disk, not display magic —
/// the Finder hides them, so they are easy to miss when typing a path by hand.
const AUTO_ADD_DIRS: &[&str] = &[
    "Automatically Add to Music.localized",
    "Automatically Add to Music",
    // Pre-Catalina libraries that were upgraded in place keep the old name.
    "Automatically Add to iTunes.localized",
    "Automatically Add to iTunes",
];

/// Media-library roots, relative to the home directory.
const MEDIA_ROOTS: &[&str] = &[
    "Music/Music/Media.localized",
    "Music/Music/Media",
    "Music/iTunes/iTunes Media",
];

/// Every path that could be the watched folder, in preference order.
pub fn candidate_paths(home: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for root in MEDIA_ROOTS {
        for dir in AUTO_ADD_DIRS {
            out.push(home.join(root).join(dir));
        }
    }
    out
}

/// The first candidate that exists, or `None` if Music has never set one up.
pub fn find_auto_add_dir(home: &Path) -> Option<PathBuf> {
    candidate_paths(home).into_iter().find(|p| p.is_dir())
}

/// Locate the watched folder for the current user.
pub fn locate() -> Result<PathBuf> {
    if !cfg!(target_os = "macos") {
        bail!("--music is macOS-only (Apple Music has no library to add to here)");
    }

    let home = directories::BaseDirs::new()
        .context("could not determine the home directory")?
        .home_dir()
        .to_path_buf();

    find_auto_add_dir(&home).with_context(|| {
        format!(
            "could not find Apple Music's watched import folder under {}.\n\
             Open Music once so it creates its library, then try again.",
            home.join("Music").display()
        )
    })
}

/// Copy `tracks` into the watched folder. Returns how many were copied.
pub fn add_tracks(auto_add: &Path, tracks: &[PathBuf]) -> Result<usize> {
    let mut copied = 0;
    for track in tracks {
        let name = track
            .file_name()
            .with_context(|| format!("{} has no filename", track.display()))?;
        let dest = auto_add.join(name);

        std::fs::copy(track, &dest).with_context(|| {
            format!(
                "could not copy {} into {}",
                track.display(),
                auto_add.display()
            )
        })?;
        copied += 1;
    }
    Ok(copied)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_cover_both_music_and_legacy_itunes_layouts() {
        let paths = candidate_paths(Path::new("/Users/x"));
        let joined: Vec<String> = paths.iter().map(|p| p.display().to_string()).collect();

        assert!(
            joined
                .iter()
                .any(|p| p.contains("Music/Music/Media.localized"))
        );
        assert!(joined.iter().any(|p| p.contains("iTunes/iTunes Media")));
        assert!(
            joined
                .iter()
                .any(|p| p.ends_with("Automatically Add to Music.localized"))
        );
        assert!(
            joined
                .iter()
                .any(|p| p.ends_with("Automatically Add to iTunes.localized"))
        );
    }

    #[test]
    fn the_modern_localized_path_is_preferred() {
        // Order matters: an upgraded library can have both, and the current
        // one is where Music actually watches.
        let first = candidate_paths(Path::new("/Users/x"))[0]
            .display()
            .to_string();
        assert!(
            first.ends_with("Music/Music/Media.localized/Automatically Add to Music.localized")
        );
    }

    #[test]
    fn finds_an_existing_watched_folder() {
        let home = tempfile::tempdir().unwrap();
        let watched = home
            .path()
            .join("Music/Music/Media.localized/Automatically Add to Music.localized");
        std::fs::create_dir_all(&watched).unwrap();

        assert_eq!(find_auto_add_dir(home.path()), Some(watched));
    }

    #[test]
    fn returns_none_when_music_has_no_library() {
        let home = tempfile::tempdir().unwrap();
        assert_eq!(find_auto_add_dir(home.path()), None);
    }

    #[test]
    fn a_file_at_the_candidate_path_does_not_count_as_the_folder() {
        let home = tempfile::tempdir().unwrap();
        let parent = home.path().join("Music/Music/Media.localized");
        std::fs::create_dir_all(&parent).unwrap();
        std::fs::write(parent.join("Automatically Add to Music.localized"), b"").unwrap();

        assert_eq!(find_auto_add_dir(home.path()), None);
    }

    #[test]
    fn add_tracks_copies_and_leaves_the_originals() {
        let src = tempfile::tempdir().unwrap();
        let watched = tempfile::tempdir().unwrap();

        let a = src.path().join("01 - One.m4a");
        let b = src.path().join("02 - Two.m4a");
        std::fs::write(&a, b"aaa").unwrap();
        std::fs::write(&b, b"bbb").unwrap();

        let n = add_tracks(watched.path(), &[a.clone(), b.clone()]).unwrap();

        assert_eq!(n, 2);
        assert!(a.exists() && b.exists(), "originals must survive the copy");
        assert_eq!(
            std::fs::read(watched.path().join("01 - One.m4a")).unwrap(),
            b"aaa"
        );
    }

    #[test]
    fn a_missing_source_track_is_reported_with_its_path() {
        let watched = tempfile::tempdir().unwrap();
        let missing = PathBuf::from("/nonexistent/01 - Gone.m4a");

        let err = add_tracks(watched.path(), &[missing])
            .unwrap_err()
            .to_string();
        assert!(err.contains("01 - Gone.m4a"), "{err}");
    }
}
