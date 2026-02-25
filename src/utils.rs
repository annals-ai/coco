//! This has all the utility functions that Coco uses
use std::{fs::File, io::Write, path::Path, process::exit, thread};

use iced::widget::image::Handle;
use icns::IconFamily;
use image::RgbaImage;
use objc2::AnyThread;
use objc2_app_kit::{NSBitmapFormat, NSBitmapImageRep, NSImageRep, NSWorkspace};
use objc2_core_foundation::CGSize;
use objc2_foundation::{NSString, NSURL};

const ICON_DECODE_TARGET_PX: u32 = 128;

/// The default error log path (works only on unix systems, and must be changed for windows
/// support)
const ERR_LOG_PATH: &str = "/tmp/rustscan-err.log";

/// This logs an error to the error log file
pub(crate) fn log_error(msg: &str) {
    eprintln!("{msg}");
    if let Ok(mut file) = File::options().create(true).append(true).open(ERR_LOG_PATH) {
        let _ = file.write_all(msg.as_bytes()).ok();
    }
}

/// This logs an error to the error log file, and exits the program
pub(crate) fn log_error_and_exit(msg: &str) -> ! {
    log_error(msg);
    exit(-1)
}

/// This converts an icns file to an iced image handle
pub(crate) fn handle_from_icns(path: &Path) -> Option<Handle> {
    let data = std::fs::read(path).ok()?;
    let family = IconFamily::read(std::io::Cursor::new(&data)).ok()?;

    let icon_type = family
        .available_icons()
        .into_iter()
        .filter(|t| !t.is_mask())
        .max_by_key(|t| {
            let w = t.pixel_width() as u64;
            let h = t.pixel_height() as u64;
            w * h
        })?;

    let icon = family.get_icon_with_type(icon_type).ok()?;
    let image = RgbaImage::from_raw(
        icon.width() as u32,
        icon.height() as u32,
        icon.data().to_vec(),
    )?;
    Some(Handle::from_rgba(
        image.width(),
        image.height(),
        image.into_raw(),
    ))
}

/// Load an application icon via NSWorkspace, which supports all icon formats
/// including Asset Catalogs (.car), .icns, and custom icons.
///
/// Picks the smallest available bitmap representation (>= target px) to avoid
/// decoding huge 1024x1024 TIFF data. Falls back to TIFFRepresentation
/// only if no suitable bitmap rep is found.
pub(crate) fn icon_from_workspace(app_path: &Path) -> Option<Handle> {
    let path_str = NSString::from_str(&app_path.to_string_lossy());
    let workspace = NSWorkspace::sharedWorkspace();
    let ns_image = workspace.iconForFile(&path_str);

    ns_image.setSize(CGSize {
        width: ICON_DECODE_TARGET_PX as f64,
        height: ICON_DECODE_TARGET_PX as f64,
    });

    // Try to find a small NSBitmapImageRep directly from representations.
    let reps = ns_image.representations();
    let bitmap = pick_best_bitmap_rep(&reps).or_else(|| {
        // Fallback: decode from TIFF (slower for large icons).
        let tiff_data = ns_image.TIFFRepresentation()?;
        NSBitmapImageRep::initWithData(NSBitmapImageRep::alloc(), &tiff_data)
    })?;

    let width = bitmap.pixelsWide() as u32;
    let height = bitmap.pixelsHigh() as u32;
    if width == 0 || height == 0 {
        return None;
    }

    let spp = bitmap.samplesPerPixel() as usize;
    if spp != 3 && spp != 4 {
        return None;
    }

    let bps = bitmap.bitsPerSample() as usize;
    let bps_bytes = bps / 8;
    if bps_bytes == 0 || (bps != 8 && bps != 16) {
        return None;
    }

    let bytes_per_row = bitmap.bytesPerRow() as usize;
    let fmt = bitmap.bitmapFormat();
    let is_float = fmt.contains(NSBitmapFormat::FloatingPointSamples);
    let alpha_first = fmt.contains(NSBitmapFormat::AlphaFirst);
    let non_premul = fmt.contains(NSBitmapFormat::AlphaNonpremultiplied);

    let raw_ptr = bitmap.bitmapData();
    if raw_ptr.is_null() {
        return None;
    }

    // SAFETY: bitmapData is valid while `bitmap` is alive.
    let raw = unsafe { std::slice::from_raw_parts(raw_ptr, bytes_per_row * height as usize) };

    // Read one sample as 0-255.
    let sample = |off: usize| -> u8 {
        if bps == 8 {
            raw[off]
        } else {
            let v = u16::from_ne_bytes([raw[off], raw[off + 1]]);
            if is_float {
                (half_to_f32(v).clamp(0.0, 1.0) * 255.0) as u8
            } else {
                (v >> 8) as u8
            }
        }
    };

    // Sample directly at a retina-friendly target resolution to avoid blurry
    // upscaling in the result list (38px logical can be ~76px physical).
    const TARGET: u32 = ICON_DECODE_TARGET_PX;
    let out_w = TARGET.min(width);
    let out_h = TARGET.min(height);
    let px_bytes = spp * bps_bytes;
    let mut rgba_buf = Vec::with_capacity((out_w * out_h * 4) as usize);

    for dy in 0..out_h {
        let sy = (dy as u64 * height as u64 / out_h as u64) as usize;
        let row = sy * bytes_per_row;
        for dx in 0..out_w {
            let sx = (dx as u64 * width as u64 / out_w as u64) as usize;
            let o = row + sx * px_bytes;
            let s = bps_bytes;

            let (r, g, b, a) = if spp == 4 {
                if alpha_first {
                    (
                        sample(o + s),
                        sample(o + 2 * s),
                        sample(o + 3 * s),
                        sample(o),
                    )
                } else {
                    (
                        sample(o),
                        sample(o + s),
                        sample(o + 2 * s),
                        sample(o + 3 * s),
                    )
                }
            } else {
                (sample(o), sample(o + s), sample(o + 2 * s), 255u8)
            };

            // Un-premultiply alpha.
            if spp == 4 && !non_premul && a > 0 && a < 255 {
                let af = a as f32;
                rgba_buf.extend_from_slice(&[
                    ((r as f32 * 255.0 / af) as u8).min(255),
                    ((g as f32 * 255.0 / af) as u8).min(255),
                    ((b as f32 * 255.0 / af) as u8).min(255),
                    a,
                ]);
            } else {
                rgba_buf.extend_from_slice(&[r, g, b, a]);
            }
        }
    }

    Some(Handle::from_rgba(out_w, out_h, rgba_buf))
}

/// Pick the smallest NSBitmapImageRep with pixelsWide >= target from the
/// representations array. Returns None if no bitmap rep is found.
fn pick_best_bitmap_rep(
    reps: &objc2_foundation::NSArray<NSImageRep>,
) -> Option<objc2::rc::Retained<NSBitmapImageRep>> {
    let target = ICON_DECODE_TARGET_PX as isize;
    let mut best_ge_target: Option<(isize, objc2::rc::Retained<NSBitmapImageRep>)> = None;
    let mut best_under_target: Option<(isize, objc2::rc::Retained<NSBitmapImageRep>)> = None;

    for rep in reps {
        // Try to downcast NSImageRep → NSBitmapImageRep
        if let Ok(bitmap) = rep.downcast::<NSBitmapImageRep>() {
            let w = bitmap.pixelsWide();
            if w <= 0 {
                continue;
            }

            if w >= target {
                let replace = best_ge_target.as_ref().is_none_or(|(bw, _)| w < *bw);
                if replace {
                    best_ge_target = Some((w, bitmap));
                }
            } else {
                let replace = best_under_target.as_ref().is_none_or(|(bw, _)| w > *bw);
                if replace {
                    best_under_target = Some((w, bitmap));
                }
            }
        }
    }
    best_ge_target.or(best_under_target).map(|(_, b)| b)
}

/// Convert IEEE 754 half-precision float (16-bit) to f32.
fn half_to_f32(h: u16) -> f32 {
    let sign = ((h >> 15) & 1) as u32;
    let exp = ((h >> 10) & 0x1F) as u32;
    let mant = (h & 0x3FF) as u32;
    if exp == 0 {
        let f = (mant as f32) * (1.0 / 16777216.0); // 2^-24
        if sign == 1 { -f } else { f }
    } else if exp == 31 {
        if mant == 0 {
            if sign == 1 {
                f32::NEG_INFINITY
            } else {
                f32::INFINITY
            }
        } else {
            f32::NAN
        }
    } else {
        f32::from_bits((sign << 31) | ((exp + 112) << 23) | (mant << 13))
    }
}

/// Open the settings file with the system default editor
pub fn open_settings() {
    thread::spawn(move || {
        NSWorkspace::new().openURL(&NSURL::fileURLWithPath(
            &objc2_foundation::NSString::from_str(
                &(std::env::var("HOME").unwrap_or("".to_string()) + "/.config/coco/config.toml"),
            ),
        ));
    });
}

/// Open a provided URL (Platform specific)
pub fn open_url(url: &str) {
    let url = url.to_owned();
    thread::spawn(move || {
        NSWorkspace::new().openURL(
            &NSURL::URLWithString_relativeToURL(&objc2_foundation::NSString::from_str(&url), None)
                .unwrap(),
        );
    });
}

pub fn is_valid_url(s: &str) -> bool {
    s.ends_with(".com")
        || s.ends_with(".net")
        || s.ends_with(".org")
        || s.ends_with(".edu")
        || s.ends_with(".gov")
        || s.ends_with(".io")
        || s.ends_with(".co")
        || s.ends_with(".me")
        || s.ends_with(".app")
        || s.ends_with(".dev")
}
