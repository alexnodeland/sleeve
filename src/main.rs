use anyhow::Result;
use clap::Parser;

use sleeve::cli::Cli;
use sleeve::config::{Config, FileConfig, config_path};
use sleeve::pipeline;

fn main() -> Result<()> {
    let cli = Cli::parse();

    let file = match config_path() {
        Some(p) => FileConfig::load(&p)?,
        None => FileConfig::default(),
    };

    let cfg = Config::resolve(
        &file,
        cli.dest.clone(),
        cli.format,
        cli.bitrate.clone(),
        cli.genre.clone(),
        cli.music,
        cli.keep_full,
    );

    let outcome = pipeline::run(&cli, &cfg)?;

    if !outcome.tracks.is_empty() {
        eprintln!("\n{}", outcome.album_dir.display());
        if outcome.cover.is_some() {
            eprintln!("cover.jpg written alongside (this format cannot embed art)");
        }
    }
    Ok(())
}
