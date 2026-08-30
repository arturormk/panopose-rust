use std::collections::VecDeque;

use image::{DynamicImage, GrayImage, Luma, RgbaImage, imageops::FilterType};
use serde::{Deserialize, Serialize};

use crate::{
    error::{PanoposeError, Result},
    export::validate_equirectangular_dimensions,
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SkyRemovalSettings {
    pub sensitivity: f64,
}

impl Default for SkyRemovalSettings {
    fn default() -> Self {
        Self { sensitivity: 0.55 }
    }
}

pub fn detect_sky_alpha_mask(
    source: &DynamicImage,
    settings: SkyRemovalSettings,
) -> Result<GrayImage> {
    validate_equirectangular_dimensions(source.width(), source.height())?;
    if source.width() == 0 || source.height() == 0 {
        return Err(PanoposeError::InvalidEquirectangularDimensions {
            width: source.width(),
            height: source.height(),
        });
    }

    let source = source.to_rgba8();
    let width = source.width();
    let height = source.height();
    let mut candidates = vec![false; (width * height) as usize];

    for y in 0..height {
        for x in 0..width {
            let pixel = source.get_pixel(x, y).0;
            candidates[index(width, x, y)] =
                is_sky_candidate(pixel[0], pixel[1], pixel[2], settings);
        }
    }

    let mut sky = connected_sky_from_top(width, height, &candidates);
    close_small_holes(width, height, &mut sky);
    Ok(feathered_alpha_mask(width, height, &sky))
}

pub fn preview_sky_removed(
    source: &DynamicImage,
    settings: SkyRemovalSettings,
    max_width: u32,
) -> Result<RgbaImage> {
    let mask = detect_sky_alpha_mask(source, settings)?;
    let mut preview = source.to_rgba8();
    for y in 0..preview.height() {
        for x in 0..preview.width() {
            preview.get_pixel_mut(x, y).0[3] = mask.get_pixel(x, y).0[0];
        }
    }

    if max_width > 0 && preview.width() > max_width {
        let height = ((preview.height() as f64 * max_width as f64 / preview.width() as f64).round()
            as u32)
            .max(1);
        Ok(image::imageops::resize(
            &preview,
            max_width,
            height,
            FilterType::Triangle,
        ))
    } else {
        Ok(preview)
    }
}

fn is_sky_candidate(r: u8, g: u8, b: u8, settings: SkyRemovalSettings) -> bool {
    let sensitivity = settings.sensitivity.clamp(-1.0, 1.0);
    let rf = r as f64 / 255.0;
    let gf = g as f64 / 255.0;
    let bf = b as f64 / 255.0;
    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let chroma = max - min;
    let saturation = if max <= f64::EPSILON {
        0.0
    } else {
        chroma / max
    };
    let hue = hue_degrees(rf, gf, bf, max, chroma);

    let strictness = (-sensitivity).max(0.0);
    let blue_min_value = if sensitivity >= 0.0 {
        0.36 - sensitivity * 0.14
    } else {
        0.36 + strictness * 0.24
    };
    let blue_min_saturation = if sensitivity >= 0.0 {
        0.16 - sensitivity * 0.08
    } else {
        0.16 + strictness * 0.12
    };
    let (blue_min_hue, blue_max_hue) = if sensitivity >= 0.0 {
        (175.0 - sensitivity * 18.0, 245.0 + sensitivity * 18.0)
    } else {
        (195.0 + strictness * 10.0, 225.0 - strictness * 5.0)
    };
    let blue_sky = max >= blue_min_value
        && saturation >= blue_min_saturation
        && hue >= blue_min_hue
        && hue <= blue_max_hue
        && bf >= rf * (1.02 - sensitivity * 0.04)
        && bf >= gf * (0.9 - sensitivity * 0.04);

    let white_min_value = if sensitivity >= 0.0 {
        0.72 - sensitivity * 0.20
    } else {
        0.72 + strictness * 0.20
    };
    let white_max_saturation = if sensitivity >= 0.0 {
        0.18 + sensitivity * 0.18
    } else {
        0.18 - strictness * 0.12
    };
    let bright_white_sky = max >= white_min_value && saturation <= white_max_saturation;

    blue_sky || bright_white_sky
}

fn hue_degrees(r: f64, g: f64, b: f64, max: f64, chroma: f64) -> f64 {
    if chroma <= f64::EPSILON {
        return 0.0;
    }
    let hue = if (max - r).abs() <= f64::EPSILON {
        60.0 * ((g - b) / chroma).rem_euclid(6.0)
    } else if (max - g).abs() <= f64::EPSILON {
        60.0 * ((b - r) / chroma + 2.0)
    } else {
        60.0 * ((r - g) / chroma + 4.0)
    };
    hue.rem_euclid(360.0)
}

fn connected_sky_from_top(width: u32, height: u32, candidates: &[bool]) -> Vec<bool> {
    let mut sky = vec![false; candidates.len()];
    let mut queue = VecDeque::new();
    let seed_rows = (height / 32).clamp(1, 48);

    for y in 0..seed_rows {
        for x in 0..width {
            let idx = index(width, x, y);
            if candidates[idx] && !sky[idx] {
                sky[idx] = true;
                queue.push_back((x, y));
            }
        }
    }

    while let Some((x, y)) = queue.pop_front() {
        let neighbors = [
            ((x + width - 1) % width, y),
            ((x + 1) % width, y),
            (x, y.saturating_sub(1)),
            (x, (y + 1).min(height - 1)),
        ];
        for (nx, ny) in neighbors {
            let idx = index(width, nx, ny);
            if candidates[idx] && !sky[idx] {
                sky[idx] = true;
                queue.push_back((nx, ny));
            }
        }
    }

    sky
}

fn close_small_holes(width: u32, height: u32, sky: &mut [bool]) {
    if width < 3 || height < 3 {
        return;
    }
    let original = sky.to_vec();
    for y in 1..height - 1 {
        for x in 0..width {
            let idx = index(width, x, y);
            if original[idx] {
                continue;
            }
            let mut sky_neighbors = 0;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let nx = (x as i32 + dx).rem_euclid(width as i32) as u32;
                    let ny = (y as i32 + dy) as u32;
                    if original[index(width, nx, ny)] {
                        sky_neighbors += 1;
                    }
                }
            }
            if sky_neighbors >= 7 {
                sky[idx] = true;
            }
        }
    }
}

fn feathered_alpha_mask(width: u32, height: u32, sky: &[bool]) -> GrayImage {
    let mut mask = GrayImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let idx = index(width, x, y);
            let alpha = if sky[idx] {
                0
            } else if touches_sky(width, height, sky, x, y, 1) {
                128
            } else {
                255
            };
            mask.put_pixel(x, y, Luma([alpha]));
        }
    }
    mask
}

fn touches_sky(width: u32, height: u32, sky: &[bool], x: u32, y: u32, radius: i32) -> bool {
    for dy in -radius..=radius {
        let ny = y as i32 + dy;
        if ny < 0 || ny >= height as i32 {
            continue;
        }
        for dx in -radius..=radius {
            let nx = (x as i32 + dx).rem_euclid(width as i32) as u32;
            if sky[index(width, nx, ny as u32)] {
                return true;
            }
        }
    }
    false
}

fn index(width: u32, x: u32, y: u32) -> usize {
    (y * width + x) as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    #[test]
    fn blue_sky_over_dark_terrain_becomes_transparent() {
        let source = test_panorama(|_, y| {
            if y < 8 {
                Rgba([95, 160, 235, 255])
            } else {
                Rgba([40, 55, 35, 255])
            }
        });
        let mask = detect_sky_alpha_mask(
            &DynamicImage::ImageRgba8(source),
            SkyRemovalSettings::default(),
        )
        .unwrap();

        assert_eq!(mask.get_pixel(10, 2).0[0], 0);
        assert_eq!(mask.get_pixel(10, 14).0[0], 255);
    }

    #[test]
    fn connected_white_cloud_inside_sky_is_removed() {
        let source = test_panorama(|x, y| {
            if y < 8 && (8..14).contains(&x) && (3..6).contains(&y) {
                Rgba([240, 242, 238, 255])
            } else if y < 8 {
                Rgba([95, 160, 235, 255])
            } else {
                Rgba([40, 55, 35, 255])
            }
        });
        let mask = detect_sky_alpha_mask(
            &DynamicImage::ImageRgba8(source),
            SkyRemovalSettings::default(),
        )
        .unwrap();

        assert_eq!(mask.get_pixel(10, 4).0[0], 0);
    }

    #[test]
    fn disconnected_blue_foreground_stays_opaque() {
        let source = test_panorama(|x, y| {
            if y < 7 {
                Rgba([95, 160, 235, 255])
            } else if (10..14).contains(&x) && (11..14).contains(&y) {
                Rgba([95, 160, 235, 255])
            } else {
                Rgba([40, 55, 35, 255])
            }
        });
        let mask = detect_sky_alpha_mask(
            &DynamicImage::ImageRgba8(source),
            SkyRemovalSettings::default(),
        )
        .unwrap();

        assert_eq!(mask.get_pixel(11, 12).0[0], 255);
    }

    #[test]
    fn negative_sensitivity_rejects_ambiguous_blue_connected_regions() {
        let source = test_panorama(|_, y| {
            if y < 8 {
                Rgba([92, 116, 132, 255])
            } else {
                Rgba([40, 55, 35, 255])
            }
        });
        let minimum_mask = detect_sky_alpha_mask(
            &DynamicImage::ImageRgba8(source.clone()),
            SkyRemovalSettings { sensitivity: 0.0 },
        )
        .unwrap();
        let stricter_mask = detect_sky_alpha_mask(
            &DynamicImage::ImageRgba8(source),
            SkyRemovalSettings { sensitivity: -1.0 },
        )
        .unwrap();

        assert_eq!(minimum_mask.get_pixel(10, 2).0[0], 0);
        assert_eq!(stricter_mask.get_pixel(10, 2).0[0], 255);
    }

    #[test]
    fn sky_connected_across_horizontal_seam_is_removed() {
        let source = test_panorama(|x, y| {
            if y < 8 && !(x == 0 || x == 31) {
                Rgba([40, 55, 35, 255])
            } else if y < 8 {
                Rgba([95, 160, 235, 255])
            } else {
                Rgba([40, 55, 35, 255])
            }
        });
        let mask = detect_sky_alpha_mask(
            &DynamicImage::ImageRgba8(source),
            SkyRemovalSettings::default(),
        )
        .unwrap();

        assert_eq!(mask.get_pixel(0, 4).0[0], 0);
        assert_eq!(mask.get_pixel(31, 4).0[0], 0);
    }

    fn test_panorama(mut color: impl FnMut(u32, u32) -> Rgba<u8>) -> RgbaImage {
        let mut image = RgbaImage::new(32, 16);
        for y in 0..16 {
            for x in 0..32 {
                image.put_pixel(x, y, color(x, y));
            }
        }
        image
    }
}
