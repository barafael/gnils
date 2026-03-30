use bevy::asset::RenderAssetUsages;
use bevy::image::Image;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::constants::*;

/// Given the ship's relative rotation angle, compute which two frames to blend
/// and the blend factor. Returns (frame1_index, frame2_index, blend_factor).
/// Port of the Python algorithm from player.py change_angle().
pub fn compute_blend_frames(rel_rot: f64) -> (usize, usize, f64) {
    let img1 = ((rel_rot + 22.5) / 45.0 - 0.49).round() as i32 % 8;
    let img2 = (rel_rot / 45.0 - 0.49).round() as i32 % 8;

    let img1 = ((img1 % 8) + 8) as usize % 8;
    let mut img2 = ((img2 % 8) + 8) as usize % 8;

    let f;
    if img1 == img2 {
        img2 = (img2 + 1) % 8;
        f = (rel_rot - img1 as f64 * 45.0) / 45.0;
    } else {
        f = ((img2 + 1) as f64 * 45.0 - rel_rot) / 45.0;
    }

    (img1, img2, f.clamp(0.0, 1.0))
}

/// Extract a single 40x33 frame from a 320x33 ship sprite strip.
pub fn extract_frame(strip: &Image, frame_index: usize) -> Image {
    let fw = SHIP_FRAME_WIDTH as usize;
    let fh = SHIP_FRAME_HEIGHT as usize;
    let strip_w = strip.width() as usize;
    let bpp = 4; // RGBA

    let mut data = vec![0u8; fw * fh * bpp];
    let x_offset = frame_index * fw;

    let strip_data = strip.data.as_ref().unwrap();
    for y in 0..fh {
        let src_start = (y * strip_w + x_offset) * bpp;
        let dst_start = y * fw * bpp;
        data[dst_start..dst_start + fw * bpp]
            .copy_from_slice(&strip_data[src_start..src_start + fw * bpp]);
    }

    Image::new(
        Extent3d {
            width: fw as u32,
            height: fh as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::all(),
    )
}

/// Blend two frames together. frame2 is blended over frame1 with the given factor.
/// factor=0 means fully frame1, factor=1 means fully frame2.
/// This matches the Python: frame2 is blitted with alpha=round(255*f) over frame1.
pub fn blend_frames(frame1: &Image, frame2: &Image, factor: f64) -> Image {
    let fw = SHIP_FRAME_WIDTH as usize;
    let fh = SHIP_FRAME_HEIGHT as usize;
    let bpp = 4;
    let alpha_overlay = (255.0 * factor).round() as u32;

    let mut data = vec![0u8; fw * fh * bpp];

    let f1_data = frame1.data.as_ref().unwrap();
    let f2_data = frame2.data.as_ref().unwrap();

    for i in 0..(fw * fh) {
        let idx = i * bpp;
        let r2 = f2_data[idx] as u32;
        let g2 = f2_data[idx + 1] as u32;
        let b2 = f2_data[idx + 2] as u32;
        let a2 = f2_data[idx + 3] as u32;

        let is_black = r2 == 0 && g2 == 0 && b2 == 0;
        let effective_a2 = if is_black {
            0
        } else {
            (a2 * alpha_overlay) / 255
        };

        let r1 = f1_data[idx] as u32;
        let g1 = f1_data[idx + 1] as u32;
        let b1 = f1_data[idx + 2] as u32;
        let a1 = f1_data[idx + 3] as u32;

        let out_a = effective_a2 + a1 * (255 - effective_a2) / 255;
        if out_a > 0 {
            data[idx] = ((r2 * effective_a2 + r1 * a1 * (255 - effective_a2) / 255) / out_a) as u8;
            data[idx + 1] =
                ((g2 * effective_a2 + g1 * a1 * (255 - effective_a2) / 255) / out_a) as u8;
            data[idx + 2] =
                ((b2 * effective_a2 + b1 * a1 * (255 - effective_a2) / 255) / out_a) as u8;
            data[idx + 3] = out_a as u8;
        }
    }

    Image::new(
        Extent3d {
            width: fw as u32,
            height: fh as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::all(),
    )
}
