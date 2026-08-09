//! Split a chapter-marked YouTube video into tagged audio tracks.
//!
//! The pipeline is four steps, and the order matters:
//!
//! 1. [`probe`] asks yt-dlp for the video's JSON, which carries the uploader's
//!    chapter list. No chapters means no album, and the run stops there.
//! 2. [`encode`] pulls the best audio stream **once**, then cuts each chapter
//!    out of that single download. Cutting from the source rather than from an
//!    already-transcoded intermediate keeps every track one generation from
//!    the original.
//! 3. [`cover`] turns the video thumbnail into square art.
//! 4. [`music`] optionally hands the finished tracks to Apple Music.
//!
//! Everything that shells out is split into a pure `*_args` function returning
//! the argv and a thin runner in [`tools`]. The arg builders are where the
//! non-obvious ffmpeg knowledge lives, so they are the part worth testing.

pub mod cli;
pub mod config;
pub mod cover;
pub mod encode;
pub mod music;
pub mod naming;
pub mod pipeline;
pub mod probe;
pub mod tools;

pub use config::{Config, Format};
pub use probe::{Chapter, VideoInfo};
