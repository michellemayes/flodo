//! The app icon, drawn in code: a check mark in the clouds.
//!
//! Keeping the artwork as a rasteriser rather than a committed bitmap means
//! the window icon, the macOS `.icns`, and the README image all come from one
//! definition, and every size is rendered at its own resolution instead of
//! being scaled down from a single large image — which is what keeps the
//! check mark readable at 16px.
//!
//! The PNG writer stores its pixels uncompressed. That is a few lines instead
//! of a deflate implementation, and the only consumer is the build pipeline,
//! which re-encodes through `sips` on the way into the `.icns`.

use std::io;
use std::path::{Path, PathBuf};

/// Geometry is written against a 1024px square and scaled to whatever size is
/// asked for.
const CANVAS: f32 = 1024.0;
/// Margin around the rounded square, in the proportions macOS icons use.
const INSET: f32 = 96.0;
const RADIUS: f32 = 190.0;
/// Width of the check mark.
const STROKE: f32 = 96.0;

/// Open Color pink 5 and pink 8 — the family the default accent is drawn
/// from in `theme.rs`, so the icon and a fresh install match. Read as a sky
/// rather than a flat plate once there are clouds in front of it.
const TOP: [f32; 3] = [0xf0 as f32, 0x65 as f32, 0x95 as f32];
const BOTTOM: [f32; 3] = [0xc2 as f32, 0x25 as f32, 0x5c as f32];
const MARK: [f32; 3] = [255.0, 255.0, 255.0];

/// How far the clouds are blended toward white. Short of opaque, so the pure
/// white check mark still reads as the front-most thing in the picture.
const CLOUD_MIX: f32 = 0.62;
/// Sky left showing between a cloud and the check mark. The two are both
/// white-ish, so without a parting they would merge into one blob at the
/// sizes where it matters most.
const HALO: f32 = 34.0;

/// The check mark, in the same proportions as the one drawn on a completed
/// to-do (`ui::check_glyph`), so the icon is a picture of the app's own mark.
const CHECK: [(f32, f32); 3] = [(-185.0, 10.0), (-65.0, 140.0), (185.0, -140.0)];

/// A cloud: overlapping puffs, blended into one another, sliced off flat
/// underneath. Coordinates are relative to the centre of the canvas.
struct Cloud {
    /// Centre and radius of each puff.
    puffs: &'static [(f32, f32, f32)],
    /// The flat bottom edge.
    base: f32,
}

/// A bank across the bottom, which the elbow of the check mark sinks into, and
/// a smaller one high on the other diagonal — the mark runs bottom-left to
/// top-right, so that is where the room is — to put the mark among the clouds
/// rather than standing on them.
const CLOUDS: [Cloud; 2] = [
    Cloud {
        // Kept clear of the inside of the check's V, where a puff would show
        // through as a stray fleck rather than as weather.
        puffs: &[
            (-240.0, 185.0, 84.0),
            (-125.0, 145.0, 122.0),
            (25.0, 180.0, 92.0),
            (150.0, 200.0, 66.0),
        ],
        base: 268.0,
    },
    Cloud {
        puffs: &[
            (-272.0, -206.0, 42.0),
            (-202.0, -230.0, 60.0),
            (-128.0, -210.0, 40.0),
        ],
        base: -172.0,
    },
];

/// Sizes macOS wants in an `.iconset`, as (pixels, file name).
const ICONSET: [(u32, &str); 10] = [
    (16, "icon_16x16.png"),
    (32, "icon_16x16@2x.png"),
    (32, "icon_32x32.png"),
    (64, "icon_32x32@2x.png"),
    (128, "icon_128x128.png"),
    (256, "icon_128x128@2x.png"),
    (256, "icon_256x256.png"),
    (512, "icon_256x256@2x.png"),
    (512, "icon_512x512.png"),
    (1024, "icon_512x512@2x.png"),
];

/// Signed distance to the rounded square: negative inside.
fn plate(x: f32, y: f32) -> f32 {
    let half = (CANVAS - 2.0 * INSET) / 2.0;
    let dx = (x - CANVAS / 2.0).abs() - (half - RADIUS);
    let dy = (y - CANVAS / 2.0).abs() - (half - RADIUS);
    let (qx, qy) = (dx.max(0.0), dy.max(0.0));
    (qx * qx + qy * qy).sqrt() + dx.max(dy).min(0.0) - RADIUS
}

fn segment_distance(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (vx, vy) = (b.0 - a.0, b.1 - a.1);
    let (wx, wy) = (p.0 - a.0, p.1 - a.1);
    let len_sq = vx * vx + vy * vy;
    let t = if len_sq > 0.0 {
        ((wx * vx + wy * vy) / len_sq).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let (dx, dy) = (wx - t * vx, wy - t * vy);
    (dx * dx + dy * dy).sqrt()
}

/// Signed distance to the check mark: negative inside. Distance-to-segment
/// gives the round caps and the mitred elbow for free.
fn check(x: f32, y: f32) -> f32 {
    let p = (x - CANVAS / 2.0, y - CANVAS / 2.0);
    let d = segment_distance(p, CHECK[0], CHECK[1]).min(segment_distance(p, CHECK[1], CHECK[2]));
    d - STROKE / 2.0
}

/// Union of two distance fields with the seam rounded off, so the puffs of a
/// cloud swell into each other instead of meeting at a crease.
fn smooth_min(a: f32, b: f32, k: f32) -> f32 {
    let h = (0.5 + 0.5 * (b - a) / k).clamp(0.0, 1.0);
    b + (a - b) * h - k * h * (1.0 - h)
}

/// Signed distance to the nearest cloud: negative inside.
fn clouds(x: f32, y: f32) -> f32 {
    let p = (x - CANVAS / 2.0, y - CANVAS / 2.0);
    let mut nearest = f32::MAX;
    for cloud in &CLOUDS {
        let mut d = f32::MAX;
        for &(cx, cy, r) in cloud.puffs {
            let puff = ((p.0 - cx).powi(2) + (p.1 - cy).powi(2)).sqrt() - r;
            d = if d == f32::MAX {
                puff
            } else {
                smooth_min(d, puff, 36.0)
            };
        }
        // Intersecting with the half-plane above `base` flattens the bottom.
        nearest = nearest.min(d.max(p.1 - cloud.base));
    }
    nearest
}

fn gradient(y: f32) -> [f32; 3] {
    let t = ((y - INSET) / (CANVAS - 2.0 * INSET)).clamp(0.0, 1.0);
    [
        TOP[0] + (BOTTOM[0] - TOP[0]) * t,
        TOP[1] + (BOTTOM[1] - TOP[1]) * t,
        TOP[2] + (BOTTOM[2] - TOP[2]) * t,
    ]
}

/// The icon as straight (unmultiplied) RGBA, which is what both
/// `egui::IconData` and PNG want.
pub fn rgba(size: u32) -> Vec<u8> {
    // 4x4 supersampling. The shapes are described by distance fields, so this
    // is the whole of the anti-aliasing.
    const SUB: u32 = 4;
    let samples = (SUB * SUB) as f32;
    let scale = CANVAS / size as f32;

    let mut out = vec![0u8; (size as usize) * (size as usize) * 4];
    for y in 0..size {
        for x in 0..size {
            let (mut r, mut g, mut b, mut covered) = (0.0, 0.0, 0.0, 0.0);
            for sy in 0..SUB {
                for sx in 0..SUB {
                    let px = (x as f32 + (sx as f32 + 0.5) / SUB as f32) * scale;
                    let py = (y as f32 + (sy as f32 + 0.5) / SUB as f32) * scale;
                    if plate(px, py) > 0.0 {
                        continue;
                    }
                    let mark = check(px, py);
                    let c = if mark <= 0.0 {
                        MARK
                    } else {
                        let sky = gradient(py);
                        // Clouds behind the mark, and parting around it.
                        if clouds(px, py) <= 0.0 && mark > HALO {
                            [
                                sky[0] + (MARK[0] - sky[0]) * CLOUD_MIX,
                                sky[1] + (MARK[1] - sky[1]) * CLOUD_MIX,
                                sky[2] + (MARK[2] - sky[2]) * CLOUD_MIX,
                            ]
                        } else {
                            sky
                        }
                    };
                    r += c[0];
                    g += c[1];
                    b += c[2];
                    covered += 1.0;
                }
            }
            if covered == 0.0 {
                continue;
            }
            let i = (y as usize * size as usize + x as usize) * 4;
            out[i] = (r / covered).round() as u8;
            out[i + 1] = (g / covered).round() as u8;
            out[i + 2] = (b / covered).round() as u8;
            out[i + 3] = (255.0 * covered / samples).round() as u8;
        }
    }
    out
}

/// The icon as a PNG.
pub fn png(size: u32) -> Vec<u8> {
    encode_png(&rgba(size), size, size)
}

/// Writes `<dir>/Flodo.iconset`, ready for `iconutil -c icns`, and returns its
/// path.
pub fn write_iconset(dir: &Path) -> io::Result<PathBuf> {
    let set = dir.join("Flodo.iconset");
    std::fs::create_dir_all(&set)?;
    // Several entries share a pixel size under two names, so render once each.
    let mut rendered: Option<(u32, Vec<u8>)> = None;
    for (size, name) in ICONSET {
        let bytes = match &rendered {
            Some((s, bytes)) if *s == size => bytes.clone(),
            _ => {
                let bytes = png(size);
                rendered = Some((size, bytes.clone()));
                bytes
            }
        };
        std::fs::write(set.join(name), bytes)?;
    }
    Ok(set)
}

// ---------------------------------------------------------------------- PNG

fn encode_png(rgba: &[u8], w: u32, h: u32) -> Vec<u8> {
    // Each scanline is prefixed with its filter type; 0 means "no filter".
    let row = (w as usize) * 4;
    let mut raw = Vec::with_capacity((row + 1) * h as usize);
    for y in 0..h as usize {
        raw.push(0);
        raw.extend_from_slice(&rgba[y * row..(y + 1) * row]);
    }

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    // 8 bits per channel, colour type 6 (RGBA), no interlacing.
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);

    let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &zlib_stored(&raw));
    chunk(&mut out, b"IEND", &[]);
    out
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc = crc32(0xffff_ffff, kind);
    crc = crc32(crc, data);
    out.extend_from_slice(&(crc ^ 0xffff_ffff).to_be_bytes());
}

/// A zlib stream of stored (uncompressed) deflate blocks.
fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01]; // deflate, 32K window, no preset dictionary
    let mut rest = data;
    loop {
        let take = rest.len().min(0xffff);
        let (block, tail) = rest.split_at(take);
        let last = tail.is_empty();
        out.push(u8::from(last));
        out.extend_from_slice(&(take as u16).to_le_bytes());
        out.extend_from_slice(&(!(take as u16)).to_le_bytes());
        out.extend_from_slice(block);
        if last {
            break;
        }
        rest = tail;
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn crc32(seed: u32, data: &[u8]) -> u32 {
    let mut crc = seed;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(rgba: &[u8], size: u32, x: u32, y: u32) -> [u8; 4] {
        let i = (y as usize * size as usize + x as usize) * 4;
        [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
    }

    #[test]
    fn corners_are_transparent_and_the_middle_is_not() {
        let size = 128;
        let px = rgba(size);
        assert_eq!(pixel(&px, size, 0, 0)[3], 0, "top-left corner is painted");
        assert_eq!(
            pixel(&px, size, size - 1, size - 1)[3],
            0,
            "bottom-right corner is painted"
        );
        assert_eq!(pixel(&px, size, size / 2, size / 2)[3], 255);
    }

    #[test]
    fn the_check_mark_is_actually_on_it() {
        let size = 128;
        let px = rgba(size);
        let white = px
            .chunks_exact(4)
            .filter(|p| p[3] == 255 && p[0] > 240 && p[1] > 240 && p[2] > 240)
            .count();
        // The mark covers a few percent of the square; the exact figure is not
        // the point, only that it is neither missing nor swallowing the icon.
        let total = (size * size) as usize;
        assert!(
            white > total / 50 && white < total / 3,
            "{white} white pixels out of {total}"
        );
    }

    /// A pixel by its position relative to the centre of the canvas, which is
    /// how the artwork's own geometry is written.
    fn at(rgba: &[u8], size: u32, cx: f32, cy: f32) -> [u8; 4] {
        let to_px = |v: f32| ((CANVAS / 2.0 + v) / CANVAS * size as f32) as u32;
        pixel(rgba, size, to_px(cx), to_px(cy))
    }

    #[test]
    fn the_clouds_are_lighter_than_the_sky_and_darker_than_the_mark() {
        let size = 256;
        let px = rgba(size);
        // The middle of the bank, against bare sky on the same row — same row,
        // so the gradient is not what is being measured.
        let cloud = at(&px, size, -230.0, 170.0);
        let sky = at(&px, size, -370.0, 170.0);
        for c in 0..3 {
            assert!(
                cloud[c] > sky[c],
                "cloud {cloud:?} is no lighter than sky {sky:?}"
            );
        }
        assert!(
            cloud.iter().take(3).any(|&c| c < 240),
            "the clouds are as white as the check mark: {cloud:?}"
        );
    }

    #[test]
    fn the_clouds_part_around_the_mark() {
        let size = 256;
        let px = rgba(size);
        // Just under the elbow: inside the bank, but within the halo, so it
        // should be sky rather than cloud.
        let gap = at(&px, size, -65.0, 203.0);
        let sky = at(&px, size, -370.0, 203.0);
        for c in 0..3 {
            assert!(
                gap[c].abs_diff(sky[c]) <= 1,
                "no gap between mark and cloud: {gap:?} against sky {sky:?}"
            );
        }
    }

    #[test]
    fn the_gradient_runs_top_to_bottom() {
        let size = 64;
        let px = rgba(size);
        // Sample the left edge of the plate, clear of the check mark.
        let top = pixel(&px, size, size / 5, size / 5);
        let bottom = pixel(&px, size, size / 5, size - size / 5);
        assert!(top[0] > bottom[0], "{top:?} vs {bottom:?}");
    }

    #[test]
    fn png_is_well_formed() {
        let bytes = png(32);
        assert_eq!(
            &bytes[..8],
            &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]
        );
        assert_eq!(&bytes[12..16], b"IHDR");
        assert_eq!(&bytes[16..20], &32u32.to_be_bytes());
        assert_eq!(&bytes[20..24], &32u32.to_be_bytes());
        assert_eq!(&bytes[bytes.len() - 8..bytes.len() - 4], b"IEND");
    }

    #[test]
    fn stored_blocks_carry_every_byte() {
        // Two full blocks and a remainder, so the 64K split is exercised.
        let data: Vec<u8> = (0..140_000u32).map(|i| i as u8).collect();
        let z = zlib_stored(&data);
        let mut at = 2; // skip the zlib header
        let mut seen = Vec::new();
        loop {
            let last = z[at] == 1;
            let len = u16::from_le_bytes([z[at + 1], z[at + 2]]) as usize;
            let nlen = u16::from_le_bytes([z[at + 3], z[at + 4]]);
            assert_eq!(nlen, !(len as u16), "NLEN is not LEN's complement");
            seen.extend_from_slice(&z[at + 5..at + 5 + len]);
            at += 5 + len;
            if last {
                break;
            }
        }
        assert_eq!(seen, data);
        assert_eq!(at + 4, z.len(), "adler32 should be the only thing left");
    }
}
