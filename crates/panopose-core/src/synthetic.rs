use image::{Rgba, RgbaImage};

use crate::coords::EquirectangularMapping;

pub fn generate_validation_panorama(width: u32, height: u32) -> RgbaImage {
    let mapping = EquirectangularMapping::new(width, height, 180.0);
    let mut image = RgbaImage::new(width, height);

    for y in 0..height {
        for x in 0..width {
            let alt_az = mapping.pixel_center_to_alt_az(x, y);
            let altitude_t = ((alt_az.altitude_deg + 90.0) / 180.0).clamp(0.0, 1.0);
            let azimuth_t = alt_az.azimuth_deg / 360.0;
            let mut color = [
                (30.0 + 120.0 * azimuth_t) as u8,
                (35.0 + 170.0 * altitude_t) as u8,
                (220.0 - 100.0 * altitude_t) as u8,
                255,
            ];

            if near_interval(alt_az.altitude_deg, 0.0, 0.35) {
                color = [255, 255, 255, 255];
            } else if near_interval(alt_az.altitude_deg, 30.0, 0.25)
                || near_interval(alt_az.altitude_deg, -30.0, 0.25)
                || near_interval(alt_az.altitude_deg, 60.0, 0.25)
                || near_interval(alt_az.altitude_deg, -60.0, 0.25)
            {
                color = [180, 220, 255, 255];
            }

            if near_azimuth_multiple(alt_az.azimuth_deg, 30.0, 0.35) {
                color = [255, 220, 120, 255];
            }
            if near_azimuth_multiple(alt_az.azimuth_deg, 90.0, 0.55) {
                color = [255, 90, 90, 255];
            }
            if alt_az.altitude_deg.abs() > 87.0 {
                color = [255, 255, 255, 255];
            }

            image.put_pixel(x, y, Rgba(color));
        }
    }

    image
}

fn near_interval(value: f64, target: f64, tolerance: f64) -> bool {
    (value - target).abs() <= tolerance
}

fn near_azimuth_multiple(value: f64, interval: f64, tolerance: f64) -> bool {
    let nearest = (value / interval).round() * interval;
    let delta = (value - nearest).abs().min((value - nearest + 360.0).abs());
    delta <= tolerance
}
