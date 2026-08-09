//! Locating and running the two binaries this tool is a wrapper around.
//!
//! Both are hard requirements and both are checked up front, before anything
//! is downloaded. Discovering a missing ffmpeg *after* a 500 MB download is a
//! bad enough experience to be worth a startup probe.

use anyhow::{Context, Result, bail};
use std::ffi::OsStr;
use std::path::Path;
use std::process::{Command, Stdio};

/// An external binary the pipeline depends on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    YtDlp,
    Ffmpeg,
    Ffprobe,
}

impl Tool {
    pub const fn binary(self) -> &'static str {
        match self {
            Tool::YtDlp => "yt-dlp",
            Tool::Ffmpeg => "ffmpeg",
            Tool::Ffprobe => "ffprobe",
        }
    }

    /// What to tell a user who does not have it.
    pub const fn install_hint(self) -> &'static str {
        match self {
            Tool::YtDlp => "brew install yt-dlp   (or: pipx install yt-dlp)",
            // ffprobe ships inside the ffmpeg formula, so the hint is the same.
            Tool::Ffmpeg | Tool::Ffprobe => "brew install ffmpeg",
        }
    }
}

/// Verify every required tool is on PATH, reporting *all* missing ones at once.
///
/// Reporting them one at a time turns first-run setup into a guessing game
/// where each fix reveals the next problem.
pub fn check_available(tools: &[Tool]) -> Result<()> {
    let missing: Vec<Tool> = tools.iter().copied().filter(|t| !on_path(*t)).collect();

    if missing.is_empty() {
        return Ok(());
    }

    let mut msg = String::from("required tool(s) not found on PATH:\n");
    for tool in &missing {
        msg.push_str(&format!(
            "  {:<8} install with: {}\n",
            tool.binary(),
            tool.install_hint()
        ));
    }
    bail!(msg.trim_end().to_string())
}

fn on_path(tool: Tool) -> bool {
    Command::new(tool.binary())
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        // yt-dlp uses --version, not -version; fall back rather than special-case.
        .unwrap_or(false)
        || Command::new(tool.binary())
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
}

/// Run a tool, letting its output through to the terminal.
///
/// Used for the download, where yt-dlp's own progress bar is a better
/// experience than anything this tool could reimplement on top of piped output.
pub fn run_inherited<S: AsRef<OsStr>>(tool: Tool, args: &[S]) -> Result<()> {
    let status = Command::new(tool.binary())
        .args(args)
        .status()
        .with_context(|| format!("failed to spawn {}", tool.binary()))?;

    if !status.success() {
        bail!("{} exited with {}", tool.binary(), status);
    }
    Ok(())
}

/// Run a tool quietly and capture stdout.
///
/// On failure the *stderr* is surfaced, because that is where both ffmpeg and
/// yt-dlp explain themselves; stdout is usually empty in that case.
pub fn run_captured<S: AsRef<OsStr>>(tool: Tool, args: &[S]) -> Result<String> {
    let out = Command::new(tool.binary())
        .args(args)
        .output()
        .with_context(|| format!("failed to spawn {}", tool.binary()))?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        bail!(
            "{} exited with {}:\n{}",
            tool.binary(),
            out.status,
            stderr.trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Read one numeric stream/format field out of a media file via ffprobe.
///
/// Returns `None` when the field is absent or unparseable rather than
/// erroring — callers use this for verification, where "cannot tell" and
/// "wrong" deserve different handling.
pub fn probe_f64(path: &Path, entries: &str) -> Option<f64> {
    let args = [
        "-v",
        "error",
        "-show_entries",
        entries,
        "-of",
        "default=noprint_wrappers=1:nokey=1",
        path.to_str()?,
    ];
    let out = run_captured(Tool::Ffprobe, &args).ok()?;
    out.lines().next()?.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_has_an_install_hint() {
        for tool in [Tool::YtDlp, Tool::Ffmpeg, Tool::Ffprobe] {
            assert!(!tool.install_hint().is_empty(), "{:?}", tool.binary());
        }
    }

    #[test]
    fn ffprobe_hint_points_at_the_ffmpeg_formula() {
        // ffprobe has no formula of its own; a hint saying `brew install
        // ffprobe` would send people to a package that does not exist.
        assert_eq!(Tool::Ffprobe.install_hint(), Tool::Ffmpeg.install_hint());
    }

    #[test]
    fn check_available_names_all_missing_tools() {
        // A binary that will never exist on PATH, so the failure is stable.
        let err = check_available(&[Tool::YtDlp]).err();
        // yt-dlp may genuinely be installed on a dev machine; only assert on
        // the shape of the message when it is not.
        if let Some(err) = err {
            let msg = err.to_string();
            assert!(msg.contains("yt-dlp"), "{msg}");
            assert!(msg.contains("install with"), "{msg}");
        }
    }
}
