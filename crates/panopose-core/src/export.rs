use image::{DynamicImage, GrayImage, Luma, Rgba, RgbaImage};

use crate::{
    coords::{AltAz, EquirectangularMapping},
    error::{PanoposeError, Result},
    orientation::Orientation,
};

#[derive(Debug, Clone, Copy)]
pub struct ExportRequest {
    pub width: u32,
    pub height: u32,
    pub center_azimuth_deg: f64,
    pub orientation: Orientation,
}

pub fn validate_equirectangular_dimensions(width: u32, height: u32) -> Result<()> {
    if EquirectangularMapping::is_plausible_full_sphere(width, height) {
        Ok(())
    } else {
        Err(PanoposeError::InvalidEquirectangularDimensions { width, height })
    }
}

pub fn export_equirectangular(source: &DynamicImage, request: ExportRequest) -> Result<RgbaImage> {
    export_equirectangular_with_progress(source, request, |_, _| {})
}

pub fn export_equirectangular_with_progress(
    source: &DynamicImage,
    request: ExportRequest,
    mut progress: impl FnMut(u32, u32),
) -> Result<RgbaImage> {
    export_equirectangular_with_mask_and_progress(source, request, None, &mut progress)
}

pub fn export_equirectangular_with_mask_and_progress(
    source: &DynamicImage,
    request: ExportRequest,
    alpha_mask: Option<&GrayImage>,
    mut progress: impl FnMut(u32, u32),
) -> Result<RgbaImage> {
    validate_equirectangular_dimensions(source.width(), source.height())?;
    validate_equirectangular_dimensions(request.width, request.height)?;
    if let Some(mask) = alpha_mask {
        validate_equirectangular_dimensions(mask.width(), mask.height())?;
        if mask.width() != source.width() || mask.height() != source.height() {
            return Err(PanoposeError::InvalidEquirectangularDimensions {
                width: mask.width(),
                height: mask.height(),
            });
        }
    }

    let source = source.to_rgba8();
    let output_mapping =
        EquirectangularMapping::new(request.width, request.height, request.center_azimuth_deg);
    let mut output = RgbaImage::new(request.width, request.height);
    progress(0, request.height);

    for y in 0..request.height {
        for x in 0..request.width {
            let world_alt_az = output_mapping.pixel_center_to_alt_az(x, y);
            let source_alt_az = request.orientation.world_alt_az_to_source(world_alt_az);
            let (sx, sy) =
                viewer_texture_alt_az_to_pixel_f64(source.width(), source.height(), source_alt_az);
            let mut pixel = sample_wrapped_bilinear(&source, sx, sy);
            if let Some(mask) = alpha_mask {
                let mask_alpha = sample_wrapped_bilinear_gray(mask, sx, sy).0[0] as u16;
                pixel.0[3] = ((pixel.0[3] as u16 * mask_alpha) / 255) as u8;
            }
            output.put_pixel(x, y, pixel);
        }
        let completed_rows = y + 1;
        if completed_rows == request.height || completed_rows % 16 == 0 {
            progress(completed_rows, request.height);
        }
    }

    Ok(output)
}

pub fn apply_nadir_overlay(
    output: &mut RgbaImage,
    radius_deg: f64,
    texture: &RgbaImage,
) -> Result<()> {
    validate_equirectangular_dimensions(output.width(), output.height())?;
    if !radius_deg.is_finite() || radius_deg <= 0.0 || radius_deg > 90.0 {
        return Err(PanoposeError::InvalidNadirOverlayRadius { radius_deg });
    }
    if texture.width() == 0 || texture.height() == 0 {
        return Err(PanoposeError::InvalidEquirectangularDimensions {
            width: texture.width(),
            height: texture.height(),
        });
    }

    let width = output.width();
    let height = output.height();
    let output_mapping = EquirectangularMapping::new(width, height, 180.0);
    for y in 0..height {
        for x in 0..width {
            let alt_az = output_mapping.pixel_center_to_alt_az(x, y);
            let angular_distance_from_nadir = alt_az.altitude_deg + 90.0;
            if angular_distance_from_nadir > radius_deg {
                continue;
            }

            let radial = angular_distance_from_nadir / radius_deg;
            let azimuth = alt_az.azimuth_deg.to_radians();
            let tx = (0.5 - azimuth.sin() * radial * 0.5) * texture.width() as f64 - 0.5;
            let ty = (0.5 + azimuth.cos() * radial * 0.5) * texture.height() as f64 - 0.5;
            let overlay = sample_clamped_bilinear(texture, tx, ty);
            let base = *output.get_pixel(x, y);
            output.put_pixel(x, y, alpha_blend(base, overlay));
        }
    }

    Ok(())
}

fn sample_wrapped_bilinear(image: &RgbaImage, x: f64, y: f64) -> Rgba<u8> {
    let width = image.width() as i64;
    let height = image.height() as i64;
    let x0 = x.floor();
    let y0 = y.floor();
    let tx = x - x0;
    let ty = y - y0;

    let y0i = (y0 as i64).clamp(0, height - 1);
    let y1i = (y0 as i64 + 1).clamp(0, height - 1);
    let x0i = (x0 as i64).rem_euclid(width);
    let x1i = (x0 as i64 + 1).rem_euclid(width);

    let p00 = image.get_pixel(x0i as u32, y0i as u32).0;
    let p10 = image.get_pixel(x1i as u32, y0i as u32).0;
    let p01 = image.get_pixel(x0i as u32, y1i as u32).0;
    let p11 = image.get_pixel(x1i as u32, y1i as u32).0;

    let mut out = [0u8; 4];
    for i in 0..4 {
        let top = lerp(p00[i] as f64, p10[i] as f64, tx);
        let bottom = lerp(p01[i] as f64, p11[i] as f64, tx);
        out[i] = lerp(top, bottom, ty).round().clamp(0.0, 255.0) as u8;
    }
    Rgba(out)
}

fn sample_clamped_bilinear(image: &RgbaImage, x: f64, y: f64) -> Rgba<u8> {
    let width = image.width() as i64;
    let height = image.height() as i64;
    let x0 = x.floor();
    let y0 = y.floor();
    let tx = x - x0;
    let ty = y - y0;

    let y0i = (y0 as i64).clamp(0, height - 1);
    let y1i = (y0 as i64 + 1).clamp(0, height - 1);
    let x0i = (x0 as i64).clamp(0, width - 1);
    let x1i = (x0 as i64 + 1).clamp(0, width - 1);

    let p00 = image.get_pixel(x0i as u32, y0i as u32).0;
    let p10 = image.get_pixel(x1i as u32, y0i as u32).0;
    let p01 = image.get_pixel(x0i as u32, y1i as u32).0;
    let p11 = image.get_pixel(x1i as u32, y1i as u32).0;

    let mut out = [0u8; 4];
    for i in 0..4 {
        let top = lerp(p00[i] as f64, p10[i] as f64, tx);
        let bottom = lerp(p01[i] as f64, p11[i] as f64, tx);
        out[i] = lerp(top, bottom, ty).round().clamp(0.0, 255.0) as u8;
    }
    Rgba(out)
}

fn alpha_blend(base: Rgba<u8>, overlay: Rgba<u8>) -> Rgba<u8> {
    let alpha = overlay.0[3] as u16;
    if alpha == 255 {
        return overlay;
    }
    if alpha == 0 {
        return base;
    }

    let inverse_alpha = 255 - alpha;
    let mut out = [0u8; 4];
    for (index, channel) in out.iter_mut().enumerate().take(3) {
        *channel =
            ((overlay.0[index] as u16 * alpha + base.0[index] as u16 * inverse_alpha) / 255) as u8;
    }
    out[3] = (alpha + (base.0[3] as u16 * inverse_alpha) / 255) as u8;
    Rgba(out)
}

fn sample_wrapped_bilinear_gray(image: &GrayImage, x: f64, y: f64) -> Luma<u8> {
    let width = image.width() as i64;
    let height = image.height() as i64;
    let x0 = x.floor();
    let y0 = y.floor();
    let tx = x - x0;
    let ty = y - y0;

    let y0i = (y0 as i64).clamp(0, height - 1);
    let y1i = (y0 as i64 + 1).clamp(0, height - 1);
    let x0i = (x0 as i64).rem_euclid(width);
    let x1i = (x0 as i64 + 1).rem_euclid(width);

    let p00 = image.get_pixel(x0i as u32, y0i as u32).0[0];
    let p10 = image.get_pixel(x1i as u32, y0i as u32).0[0];
    let p01 = image.get_pixel(x0i as u32, y1i as u32).0[0];
    let p11 = image.get_pixel(x1i as u32, y1i as u32).0[0];

    let top = lerp(p00 as f64, p10 as f64, tx);
    let bottom = lerp(p01 as f64, p11 as f64, tx);
    Luma([lerp(top, bottom, ty).round().clamp(0.0, 255.0) as u8])
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

pub fn source_pixel_to_world_alt_az(
    source_mapping: EquirectangularMapping,
    orientation: Orientation,
    x: u32,
    y: u32,
) -> AltAz {
    orientation.source_alt_az_to_world(viewer_texture_pixel_center_to_alt_az(
        source_mapping.width,
        source_mapping.height,
        x,
        y,
    ))
}

pub fn viewer_texture_pixel_center_to_alt_az(width: u32, height: u32, x: u32, y: u32) -> AltAz {
    let u = (x as f64 + 0.5) / width as f64;
    let v = (y as f64 + 0.5) / height as f64;
    AltAz {
        azimuth_deg: crate::coords::normalize_degrees(90.0 - u * 360.0),
        altitude_deg: 90.0 - v * 180.0,
    }
}

fn viewer_texture_alt_az_to_pixel_f64(width: u32, height: u32, alt_az: AltAz) -> (f64, f64) {
    let u = crate::coords::normalize_degrees(90.0 - alt_az.azimuth_deg) / 360.0;
    let v = (90.0 - alt_az.altitude_deg) / 180.0;
    (u * width as f64 - 0.5, v * height as f64 - 0.5)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbaImage;

    #[test]
    fn rejects_non_two_to_one_source() {
        let bad = DynamicImage::ImageRgba8(RgbaImage::new(100, 100));
        let err = export_equirectangular(
            &bad,
            ExportRequest {
                width: 200,
                height: 100,
                center_azimuth_deg: 180.0,
                orientation: Orientation::identity(),
            },
        )
        .unwrap_err();
        assert!(matches!(
            err,
            PanoposeError::InvalidEquirectangularDimensions { .. }
        ));
    }

    #[test]
    fn identity_export_keeps_dimensions() {
        let src = DynamicImage::ImageRgba8(RgbaImage::from_pixel(16, 8, Rgba([1, 2, 3, 255])));
        let out = export_equirectangular(
            &src,
            ExportRequest {
                width: 32,
                height: 16,
                center_azimuth_deg: 180.0,
                orientation: Orientation::identity(),
            },
        )
        .unwrap();
        assert_eq!(out.dimensions(), (32, 16));
    }

    #[test]
    fn progress_reports_start_and_completion() {
        let src = DynamicImage::ImageRgba8(RgbaImage::from_pixel(16, 8, Rgba([1, 2, 3, 255])));
        let mut reports = Vec::new();
        export_equirectangular_with_progress(
            &src,
            ExportRequest {
                width: 32,
                height: 16,
                center_azimuth_deg: 180.0,
                orientation: Orientation::identity(),
            },
            |completed_rows, total_rows| reports.push((completed_rows, total_rows)),
        )
        .unwrap();

        assert_eq!(reports.first(), Some(&(0, 16)));
        assert_eq!(reports.last(), Some(&(16, 16)));
    }

    #[test]
    fn viewer_texture_mapping_matches_threejs_sphere_uvs() {
        let east = viewer_texture_pixel_center_to_alt_az(360, 180, 0, 89);
        let north = viewer_texture_pixel_center_to_alt_az(360, 180, 89, 89);
        let west = viewer_texture_pixel_center_to_alt_az(360, 180, 179, 89);
        let south = viewer_texture_pixel_center_to_alt_az(360, 180, 269, 89);

        assert!((north.altitude_deg - 0.5).abs() < 1e-9);
        assert!((east.azimuth_deg - 89.5).abs() < 1e-9);
        assert!((north.azimuth_deg - 0.5).abs() < 1e-9);
        assert!((west.azimuth_deg - 270.5).abs() < 1e-9);
        assert!((south.azimuth_deg - 180.5).abs() < 1e-9);
    }

    #[test]
    fn export_with_alpha_mask_makes_masked_pixels_transparent() {
        let src = DynamicImage::ImageRgba8(RgbaImage::from_pixel(16, 8, Rgba([1, 2, 3, 255])));
        let mut mask = GrayImage::from_pixel(16, 8, Luma([255]));
        for y in 0..4 {
            for x in 0..16 {
                mask.put_pixel(x, y, Luma([0]));
            }
        }

        let out = export_equirectangular_with_mask_and_progress(
            &src,
            ExportRequest {
                width: 16,
                height: 8,
                center_azimuth_deg: 180.0,
                orientation: Orientation::identity(),
            },
            Some(&mask),
            |_, _| {},
        )
        .unwrap();

        assert_eq!(out.get_pixel(8, 1).0[3], 0);
        assert_eq!(out.get_pixel(8, 6).0[3], 255);
    }

    #[test]
    fn nadir_overlay_replaces_pixels_inside_radius() {
        let mut output = RgbaImage::from_pixel(360, 180, Rgba([10, 20, 30, 255]));
        let texture = RgbaImage::from_pixel(8, 8, Rgba([0, 0, 0, 255]));

        apply_nadir_overlay(&mut output, 2.0, &texture).unwrap();

        assert_eq!(output.get_pixel(180, 179).0, [0, 0, 0, 255]);
    }

    #[test]
    fn nadir_overlay_leaves_pixels_outside_radius_unchanged() {
        let mut output = RgbaImage::from_pixel(360, 180, Rgba([10, 20, 30, 255]));
        let texture = RgbaImage::from_pixel(8, 8, Rgba([0, 0, 0, 255]));

        apply_nadir_overlay(&mut output, 2.0, &texture).unwrap();

        assert_eq!(output.get_pixel(180, 170).0, [10, 20, 30, 255]);
    }

    #[test]
    fn nadir_overlay_radius_uses_angular_distance_from_nadir() {
        let mut output = RgbaImage::from_pixel(360, 180, Rgba([10, 20, 30, 255]));
        let texture = RgbaImage::from_pixel(8, 8, Rgba([0, 0, 0, 255]));

        apply_nadir_overlay(&mut output, 10.0, &texture).unwrap();

        assert_eq!(output.get_pixel(180, 170).0, [0, 0, 0, 255]);
        assert_eq!(output.get_pixel(180, 169).0, [10, 20, 30, 255]);
    }

    #[test]
    fn nadir_overlay_uses_final_export_raster_orientation() {
        let mut output = RgbaImage::from_pixel(360, 180, Rgba([10, 20, 30, 255]));
        let texture = RgbaImage::from_fn(101, 101, |x, y| {
            if y < 25 {
                Rgba([200, 0, 0, 255])
            } else if x < 25 {
                Rgba([0, 0, 200, 255])
            } else if x >= 75 {
                Rgba([0, 200, 0, 255])
            } else {
                Rgba([0, 0, 0, 255])
            }
        });

        apply_nadir_overlay(&mut output, 10.0, &texture).unwrap();

        assert_eq!(output.get_pixel(180, 170).0, [200, 0, 0, 255]);
        assert_eq!(output.get_pixel(90, 170).0, [0, 0, 200, 255]);
        assert_eq!(output.get_pixel(270, 170).0, [0, 200, 0, 255]);
    }

    #[test]
    fn nadir_overlay_rejects_zero_radius() {
        let mut output = RgbaImage::from_pixel(360, 180, Rgba([10, 20, 30, 255]));
        let texture = RgbaImage::from_pixel(8, 8, Rgba([0, 0, 0, 255]));

        let err = apply_nadir_overlay(&mut output, 0.0, &texture).unwrap_err();

        assert!(matches!(
            err,
            PanoposeError::InvalidNadirOverlayRadius { .. }
        ));
        assert_eq!(output.get_pixel(180, 179).0, [10, 20, 30, 255]);
    }
}
