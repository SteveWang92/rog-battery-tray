//! Tray icons rendered at runtime: the battery percentage as digits, drawn with the system
//! font at the taskbar's DPI so they stay sharp at every scale.

use std::ptr;

use tray_icon::Icon;
use windows_sys::Win32::Graphics::Gdi::{
    ANTIALIASED_QUALITY, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CLIP_DEFAULT_PRECIS,
    CreateCompatibleDC, CreateDIBSection, CreateFontW, DEFAULT_CHARSET, DEFAULT_PITCH,
    DIB_RGB_COLORS, DeleteDC, DeleteObject, FW_BOLD, GdiFlush, OUT_TT_PRECIS, SelectObject,
    SetBkMode, SetTextColor, TRANSPARENT, TextOutW,
};
use windows_sys::Win32::UI::HiDpi::GetSystemMetricsForDpi;
use windows_sys::Win32::UI::WindowsAndMessaging::SM_CXSMICON;

const LOW_FILL: [u8; 3] = [0xC2, 0x41, 0x0C];
const CHARGING_FILL: [u8; 3] = [0x15, 0x80, 0x3D];
const GREY: [u8; 3] = [0x8A, 0x8A, 0x8A];
const WHITE: [u8; 3] = [0xFF, 0xFF, 0xFF];
const DARK_TEXT: [u8; 3] = [0x1A, 0x1A, 0x1A];

/// What one tray icon shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    Level {
        percent: u8,
        charging: bool,
        low: bool,
    },
    Asleep,
    NoDevice,
}

/// Renders `face` at the small-icon size for `dpi`. `light_taskbar` picks the digit colour
/// of the plain state so it reads against the taskbar.
pub fn render(face: Face, dpi: u32, light_taskbar: bool) -> Icon {
    let (rgba, size) = rgba(face, dpi, light_taskbar);
    Icon::from_rgba(rgba, size as u32, size as u32).expect("icon buffer matches its size")
}

/// The icon's straight-alpha RGBA pixels and its edge length.
fn rgba(face: Face, dpi: u32, light_taskbar: bool) -> (Vec<u8>, usize) {
    let size = unsafe { GetSystemMetricsForDpi(SM_CXSMICON, dpi) } as usize;
    let plain = if light_taskbar { DARK_TEXT } else { WHITE };
    let (text, fill, colour) = match face {
        Face::Level {
            percent, charging, ..
        } if charging => (percent.to_string(), Some(CHARGING_FILL), WHITE),
        Face::Level {
            percent, low: true, ..
        } => (percent.to_string(), Some(LOW_FILL), WHITE),
        Face::Level { percent, .. } => (percent.to_string(), None, plain),
        Face::Asleep => ("–".to_owned(), None, GREY),
        Face::NoDevice => ("?".to_owned(), None, GREY),
    };

    let s = size as f32;
    let (max_w, max_h) = match (face, fill) {
        (Face::Asleep | Face::NoDevice, _) => (s * 0.7, s * 0.75),
        (_, Some(_)) => (s - 2.0 * (s / 16.0).ceil(), s * 0.7),
        (_, None) => (s, s * 0.8),
    };
    let glyphs = Glyphs::fit(&text, size, max_w as usize, max_h as usize);

    let mut rgba = vec![0u8; size * size * 4];
    let radius = s * 0.19;
    let ox = (size - glyphs.width) / 2;
    let oy = (size - glyphs.height) / 2;
    for y in 0..size {
        for x in 0..size {
            let coverage =
                if (ox..ox + glyphs.width).contains(&x) && (oy..oy + glyphs.height).contains(&y) {
                    f32::from(glyphs.coverage[(y - oy) * glyphs.width + (x - ox)]) / 255.0
                } else {
                    0.0
                };
            let (rgb, alpha) = match fill {
                Some(fill) => (
                    blend(fill, colour, coverage),
                    rounded_rect_alpha(x, y, s, radius),
                ),
                None => (colour, coverage),
            };
            let pixel = &mut rgba[(y * size + x) * 4..][..4];
            pixel[..3].copy_from_slice(&rgb);
            pixel[3] = (alpha * 255.0).round() as u8;
        }
    }
    (rgba, size)
}

fn blend(base: [u8; 3], top: [u8; 3], t: f32) -> [u8; 3] {
    std::array::from_fn(|i| (f32::from(base[i]) * (1.0 - t) + f32::from(top[i]) * t).round() as u8)
}

/// Anti-aliased coverage of the pixel at (x, y) by a rounded square filling the icon.
fn rounded_rect_alpha(x: usize, y: usize, size: f32, radius: f32) -> f32 {
    let half = size / 2.0;
    let inner = half - radius;
    let dx = ((x as f32 + 0.5 - half).abs() - inner).max(0.0);
    let dy = ((y as f32 + 0.5 - half).abs() - inner).max(0.0);
    let distance = (dx * dx + dy * dy).sqrt() - radius;
    (0.5 - distance).clamp(0.0, 1.0)
}

/// Text rasterised as an 8-bit coverage mask cropped to its ink.
struct Glyphs {
    width: usize,
    height: usize,
    coverage: Vec<u8>,
}

impl Glyphs {
    /// The largest bold system-font rendering of `text` whose ink fits `max_w` × `max_h`.
    fn fit(text: &str, size: usize, max_w: usize, max_h: usize) -> Glyphs {
        let mut em = size * 3 / 2;
        loop {
            let glyphs = Glyphs::draw(text, em as i32, size * 3);
            if em <= 6 || (glyphs.width <= max_w && glyphs.height <= max_h) {
                return glyphs;
            }
            em -= 1;
        }
    }

    fn draw(text: &str, em: i32, canvas: usize) -> Glyphs {
        let wide: Vec<u16> = text.encode_utf16().collect();
        let face: Vec<u16> = "Segoe UI\0".encode_utf16().collect();
        let mut full = vec![0u8; canvas * canvas];
        unsafe {
            let dc = CreateCompatibleDC(ptr::null_mut());
            let mut info: BITMAPINFO = std::mem::zeroed();
            info.bmiHeader = BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: canvas as i32,
                biHeight: -(canvas as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..std::mem::zeroed()
            };
            let mut bits: *mut core::ffi::c_void = ptr::null_mut();
            let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, ptr::null_mut(), 0);
            let pixels = std::slice::from_raw_parts_mut(bits.cast::<u8>(), canvas * canvas * 4);
            pixels.fill(0);
            let old_bitmap = SelectObject(dc, bitmap);
            let font = CreateFontW(
                -em,
                0,
                0,
                0,
                FW_BOLD as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET.into(),
                OUT_TT_PRECIS.into(),
                CLIP_DEFAULT_PRECIS.into(),
                ANTIALIASED_QUALITY.into(),
                DEFAULT_PITCH as u32,
                face.as_ptr(),
            );
            let old_font = SelectObject(dc, font);
            SetTextColor(dc, 0x00FF_FFFF);
            SetBkMode(dc, TRANSPARENT as i32);
            let margin = (canvas / 6) as i32;
            TextOutW(dc, margin, margin, wide.as_ptr(), wide.len() as i32);
            GdiFlush();
            // White text on black: any channel is the coverage.
            for (cell, pixel) in full.iter_mut().zip(pixels.chunks_exact(4)) {
                *cell = pixel[0];
            }
            SelectObject(dc, old_font);
            DeleteObject(font);
            SelectObject(dc, old_bitmap);
            DeleteObject(bitmap);
            DeleteDC(dc);
        }
        Glyphs::crop(&full, canvas)
    }

    fn crop(full: &[u8], canvas: usize) -> Glyphs {
        let ink = |x: usize, y: usize| full[y * canvas + x] > 0;
        let rows: Vec<usize> = (0..canvas)
            .filter(|&y| (0..canvas).any(|x| ink(x, y)))
            .collect();
        let cols: Vec<usize> = (0..canvas)
            .filter(|&x| (0..canvas).any(|y| ink(x, y)))
            .collect();
        let (Some(&top), Some(&bottom), Some(&left), Some(&right)) =
            (rows.first(), rows.last(), cols.first(), cols.last())
        else {
            return Glyphs {
                width: 0,
                height: 0,
                coverage: Vec::new(),
            };
        };
        let width = right - left + 1;
        let height = bottom - top + 1;
        let coverage = (top..=bottom)
            .flat_map(|y| full[y * canvas + left..][..width].iter().copied())
            .collect();
        Glyphs {
            width,
            height,
            coverage,
        }
    }
}
