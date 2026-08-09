//! Turning a 16:9 video thumbnail into square cover art.
//!
//! Two things make this more than a centre-crop. Thumbnails are frequently
//! *already* letterboxed or pillarboxed — the uploader exported a 4:3 or
//! square graphic into a 16:9 frame — so the real image is a sub-rectangle of
//! the file. And a centre-crop of a genuine 16:9 image throws away a third of
//! it, which for a title card means cropping the text.
//!
//! So: detect the real content box, crop to it, then **pad** to square rather
//! than cropping further. Padding costs nothing but background; cropping costs
//! the picture.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::tools::{Tool, run_captured};

/// A rectangle within an image, in ffmpeg's `crop=w:h:x:y` order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CropBox {
    pub w: u32,
    pub h: u32,
    pub x: u32,
    pub y: u32,
}

/// yt-dlp args to fetch the thumbnail and normalise it to JPEG.
///
/// YouTube serves WebP now; converting at download time means the rest of the
/// pipeline only ever handles one format.
pub fn thumbnail_args(url: &str, out_template: &str) -> Vec<String> {
    vec![
        "--skip-download".into(),
        "--write-thumbnail".into(),
        "--convert-thumbnails".into(),
        "jpg".into(),
        "--no-warnings".into(),
        "--no-playlist".into(),
        "-o".into(),
        out_template.into(),
        url.into(),
    ]
}

/// ffmpeg args that make cropdetect report the content box of a still image.
///
/// `-loop 1` is load-bearing and not obvious. cropdetect accumulates across
/// frames and only prints once it has something to say; handed a single JPEG
/// frame it emits **nothing at all**, and the caller silently concludes there
/// are no bars. Looping the still gives it the frames it wants, and three is
/// enough for the estimate to settle.
pub fn cropdetect_args(input: &Path) -> Vec<String> {
    vec![
        "-hide_banner".into(),
        "-loop".into(),
        "1".into(),
        "-i".into(),
        input.to_string_lossy().into_owned(),
        // limit=24 tolerates the not-quite-black bars that JPEG compression
        // produces; round=2 keeps the result even, which some encoders need.
        "-vf".into(),
        "cropdetect=limit=24:round=2".into(),
        "-frames:v".into(),
        "3".into(),
        "-f".into(),
        "null".into(),
        "-".into(),
    ]
}

/// Pull the last `crop=w:h:x:y` out of ffmpeg's stderr.
///
/// The last one is the most-refined estimate: cropdetect emits a line per
/// frame and narrows as it goes.
pub fn parse_cropdetect(stderr: &str) -> Option<CropBox> {
    let mut found = None;
    for line in stderr.lines() {
        let Some(idx) = line.rfind("crop=") else {
            continue;
        };
        let spec = &line[idx + "crop=".len()..];
        let nums: Vec<&str> = spec.trim().split(':').collect();
        if nums.len() != 4 {
            continue;
        }
        let parsed: Option<Vec<u32>> = nums.iter().map(|n| n.trim().parse().ok()).collect();
        // `.filter` rather than a let-chain: let-chains are unstable before
        // Rust 1.88 and this crate's MSRV is 1.85.
        if let Some(v) = parsed.filter(|v| v[0] > 0 && v[1] > 0) {
            found = Some(CropBox {
                w: v[0],
                h: v[1],
                x: v[2],
                y: v[3],
            });
        }
    }
    found
}

/// The ffmpeg filter chain that produces square art.
///
/// `crop` is the detected content box; when it already covers the whole frame
/// the crop is still emitted (it is a no-op) so the filter string has one
/// shape and one set of tests.
pub fn square_filter(source_w: u32, source_h: u32, crop: Option<CropBox>) -> String {
    let c = crop.unwrap_or(CropBox {
        w: source_w,
        h: source_h,
        x: 0,
        y: 0,
    });
    let side = c.w.max(c.h);
    let pad_x = (side - c.w) / 2;
    let pad_y = (side - c.h) / 2;

    format!(
        "crop={}:{}:{}:{},pad={side}:{side}:{pad_x}:{pad_y}:black",
        c.w, c.h, c.x, c.y
    )
}

/// Full ffmpeg args to render the square cover.
pub fn render_args(input: &Path, output: &Path, filter: &str) -> Vec<String> {
    vec![
        "-v".into(),
        "error".into(),
        "-y".into(),
        "-i".into(),
        input.to_string_lossy().into_owned(),
        "-vf".into(),
        filter.into(),
        // q:v 2 is visually lossless for JPEG and keeps the art well under the
        // size where players start to complain about embedded images.
        "-q:v".into(),
        "2".into(),
        output.to_string_lossy().into_owned(),
    ]
}

/// Detect the content box of `input`, returning `None` if ffmpeg cannot tell.
pub fn detect_crop(input: &Path) -> Option<CropBox> {
    // cropdetect writes to stderr, and the `null` muxer makes ffmpeg exit
    // non-zero on some builds, so the captured-output helper is not usable
    // here — read stderr directly.
    let out = std::process::Command::new(Tool::Ffmpeg.binary())
        .args(cropdetect_args(input))
        .output()
        .ok()?;
    parse_cropdetect(&String::from_utf8_lossy(&out.stderr))
}

/// Read an image's dimensions.
pub fn dimensions(input: &Path) -> Result<(u32, u32)> {
    let args = [
        "-v",
        "error",
        "-select_streams",
        "v:0",
        "-show_entries",
        "stream=width,height",
        "-of",
        "csv=p=0",
        &input.to_string_lossy(),
    ];
    let out = run_captured(Tool::Ffprobe, &args)?;
    let line = out.lines().next().unwrap_or_default();
    let (w, h) = line
        .trim()
        .split_once(',')
        .with_context(|| format!("ffprobe gave no dimensions for {}", input.display()))?;
    Ok((w.trim().parse()?, h.trim().parse()?))
}

/// Build square cover art from a thumbnail, writing it next to the source.
pub fn build(thumbnail: &Path, out_dir: &Path) -> Result<PathBuf> {
    let (w, h) = dimensions(thumbnail)?;
    let crop = detect_crop(thumbnail);
    let filter = square_filter(w, h, crop);
    let output = out_dir.join("cover.jpg");

    crate::tools::run_inherited(Tool::Ffmpeg, &render_args(thumbnail, &output, &filter))
        .context("could not render the cover art")?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_cropdetect_line() {
        let stderr = "[Parsed_cropdetect_0 @ 0x14] x1:99 x2:1179 y1:0 y2:719 w:1080 h:720 \
                      x:100 y:0 pts:0 t:0 crop=1080:720:100:0";
        assert_eq!(
            parse_cropdetect(stderr),
            Some(CropBox {
                w: 1080,
                h: 720,
                x: 100,
                y: 0
            })
        );
    }

    #[test]
    fn takes_the_last_estimate_when_several_are_reported() {
        let stderr = "crop=1280:720:0:0\ncrop=1080:720:100:0\n";
        assert_eq!(parse_cropdetect(stderr).unwrap().w, 1080);
    }

    #[test]
    fn ignores_output_with_no_crop_line() {
        assert_eq!(parse_cropdetect("no crop here\nffmpeg version 8"), None);
    }

    #[test]
    fn ignores_a_degenerate_zero_area_crop() {
        // cropdetect reports this for an all-black frame; honouring it would
        // produce an empty cover.
        assert_eq!(parse_cropdetect("crop=0:0:0:0"), None);
    }

    #[test]
    fn pillarboxed_thumbnail_is_cropped_then_padded_to_square() {
        // The real case: a 1280x720 frame whose content is 1080x720.
        let crop = CropBox {
            w: 1080,
            h: 720,
            x: 100,
            y: 0,
        };
        let filter = square_filter(1280, 720, Some(crop));
        assert_eq!(filter, "crop=1080:720:100:0,pad=1080:1080:0:180:black");
    }

    #[test]
    fn a_full_frame_16_9_thumbnail_pads_without_losing_picture() {
        let filter = square_filter(1280, 720, None);
        // Side is the long edge, so nothing is cropped away.
        assert_eq!(filter, "crop=1280:720:0:0,pad=1280:1280:0:280:black");
    }

    #[test]
    fn an_already_square_image_is_a_no_op_filter() {
        let filter = square_filter(1000, 1000, None);
        assert_eq!(filter, "crop=1000:1000:0:0,pad=1000:1000:0:0:black");
    }

    #[test]
    fn a_tall_image_pads_horizontally() {
        let filter = square_filter(600, 900, None);
        assert_eq!(filter, "crop=600:900:0:0,pad=900:900:150:0:black");
    }

    #[test]
    fn odd_padding_rounds_down_and_never_exceeds_the_square() {
        // 101 -> side 101, pad_x = 0; the point is that (side - w)/2 can never
        // push the image outside the canvas.
        let filter = square_filter(100, 101, None);
        assert!(filter.contains("pad=101:101:0:0"), "{filter}");
    }

    #[test]
    fn render_args_overwrite_and_stay_quiet() {
        let args = render_args(Path::new("in.jpg"), Path::new("out.jpg"), "scale=1:1");
        assert!(args.contains(&"-y".to_string()), "must overwrite on re-run");
        assert!(args.windows(2).any(|w| w == ["-vf", "scale=1:1"]));
    }

    #[test]
    fn cropdetect_loops_the_still_so_it_has_frames_to_converge_on() {
        // The regression this guards: handed one frame, cropdetect prints
        // nothing, detection silently returns None, and every cover is padded
        // as though it had no bars. The bug is invisible in the output.
        let args = cropdetect_args(Path::new("thumb.jpg"));
        let loop_at = args.iter().position(|a| a == "-loop").expect("must loop");
        let input_at = args.iter().position(|a| a == "-i").unwrap();
        assert!(loop_at < input_at, "-loop is an input option");

        let frames = args
            .iter()
            .position(|a| a == "-frames:v")
            .map(|i| args[i + 1].parse::<u32>().unwrap())
            .unwrap();
        assert!(
            frames >= 2,
            "one frame produces no cropdetect output at all"
        );
    }

    #[test]
    fn thumbnail_args_request_jpeg_and_refuse_playlists() {
        let args = thumbnail_args("https://youtu.be/x", "thumb.%(ext)s");
        assert!(
            args.windows(2)
                .any(|w| w == ["--convert-thumbnails", "jpg"])
        );
        assert!(args.contains(&"--no-playlist".to_string()));
    }
}
