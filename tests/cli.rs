//! Integration tests against the built binary.
//!
//! Hermetic like the unit tests: these only exercise paths that fail or print
//! before any network call or subprocess spawn, so the suite still needs
//! neither yt-dlp nor ffmpeg.

use std::process::Command;

fn sleeve() -> Command {
    Command::new(env!("CARGO_BIN_EXE_sleeve"))
}

#[test]
fn help_lists_the_flags_the_readme_documents() {
    let out = sleeve().arg("--help").output().unwrap();
    assert!(out.status.success());

    let stdout = String::from_utf8_lossy(&out.stdout);
    for flag in [
        "--dest",
        "--format",
        "--bitrate",
        "--artist",
        "--album",
        "--year",
        "--genre",
        "--music",
        "--keep-full",
        "--no-cover",
        "--flat",
        "--dry-run",
        "--list",
    ] {
        assert!(stdout.contains(flag), "--help omits {flag}:\n{stdout}");
    }
}

#[test]
fn version_is_reported() {
    let out = sleeve().arg("--version").output().unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn a_missing_url_is_a_usage_error() {
    let out = sleeve().output().unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("URL") || stderr.contains("Usage"),
        "{stderr}"
    );
}

#[test]
fn an_unknown_format_is_rejected_before_anything_is_downloaded() {
    let out = sleeve()
        .args(["https://example.invalid/x", "--format", "flac"])
        .output()
        .unwrap();

    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    // clap rejects the value and lists what is accepted.
    assert!(stderr.contains("m4a"), "{stderr}");
}
