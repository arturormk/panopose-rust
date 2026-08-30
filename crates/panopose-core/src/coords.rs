use glam::DVec3;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AltAz {
    pub altitude_deg: f64,
    pub azimuth_deg: f64,
}

impl AltAz {
    pub const NORTH_HORIZON: Self = Self {
        altitude_deg: 0.0,
        azimuth_deg: 0.0,
    };
    pub const EAST_HORIZON: Self = Self {
        altitude_deg: 0.0,
        azimuth_deg: 90.0,
    };
    pub const SOUTH_HORIZON: Self = Self {
        altitude_deg: 0.0,
        azimuth_deg: 180.0,
    };
    pub const WEST_HORIZON: Self = Self {
        altitude_deg: 0.0,
        azimuth_deg: 270.0,
    };
    pub const ZENITH: Self = Self {
        altitude_deg: 90.0,
        azimuth_deg: 0.0,
    };
    pub const NADIR: Self = Self {
        altitude_deg: -90.0,
        azimuth_deg: 0.0,
    };

    pub fn to_unit_vector(self) -> DVec3 {
        let alt = self.altitude_deg.to_radians();
        let az = self.azimuth_deg.to_radians();
        let r = alt.cos();
        DVec3::new(r * az.sin(), alt.sin(), r * az.cos()).normalize()
    }

    pub fn from_unit_vector(v: DVec3) -> Self {
        let n = v.normalize();
        let altitude_deg = n.y.clamp(-1.0, 1.0).asin().to_degrees();
        let azimuth_deg = normalize_degrees(n.x.atan2(n.z).to_degrees());
        Self {
            altitude_deg,
            azimuth_deg,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EquirectangularMapping {
    pub width: u32,
    pub height: u32,
    pub center_azimuth_deg: f64,
}

impl EquirectangularMapping {
    pub fn new(width: u32, height: u32, center_azimuth_deg: f64) -> Self {
        Self {
            width,
            height,
            center_azimuth_deg,
        }
    }

    pub fn is_plausible_full_sphere(width: u32, height: u32) -> bool {
        if width < 2 || height < 1 {
            return false;
        }
        let ratio = width as f64 / height as f64;
        (ratio - 2.0).abs() <= 0.02
    }

    pub fn pixel_center_to_alt_az(self, x: u32, y: u32) -> AltAz {
        let u = (x as f64 + 0.5) / self.width as f64;
        let v = (y as f64 + 0.5) / self.height as f64;
        AltAz {
            azimuth_deg: normalize_degrees(self.center_azimuth_deg + (u - 0.5) * 360.0),
            altitude_deg: 90.0 - v * 180.0,
        }
    }

    pub fn alt_az_to_pixel_f64(self, alt_az: AltAz) -> (f64, f64) {
        let delta = signed_degrees(alt_az.azimuth_deg - self.center_azimuth_deg);
        let u = delta / 360.0 + 0.5;
        let v = (90.0 - alt_az.altitude_deg) / 180.0;
        (u * self.width as f64 - 0.5, v * self.height as f64 - 0.5)
    }
}

pub fn normalize_degrees(value: f64) -> f64 {
    value.rem_euclid(360.0)
}

pub fn signed_degrees(value: f64) -> f64 {
    let wrapped = (value + 180.0).rem_euclid(360.0) - 180.0;
    if wrapped == -180.0 { 180.0 } else { wrapped }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "{a} != {b}");
    }

    #[test]
    fn alt_az_cardinal_vectors_use_x_east_y_up_z_north() {
        assert_close(AltAz::NORTH_HORIZON.to_unit_vector().z, 1.0);
        assert_close(AltAz::EAST_HORIZON.to_unit_vector().x, 1.0);
        assert_close(AltAz::SOUTH_HORIZON.to_unit_vector().z, -1.0);
        assert_close(AltAz::WEST_HORIZON.to_unit_vector().x, -1.0);
        assert_close(AltAz::ZENITH.to_unit_vector().y, 1.0);
        assert_close(AltAz::NADIR.to_unit_vector().y, -1.0);
    }

    #[test]
    fn equirectangular_mapping_uses_pixel_centers() {
        let mapping = EquirectangularMapping::new(360, 180, 180.0);
        let center = mapping.pixel_center_to_alt_az(179, 89);
        assert_close(center.azimuth_deg, 179.5);
        assert_close(center.altitude_deg, 0.5);

        let (x, y) = mapping.alt_az_to_pixel_f64(center);
        assert_close(x, 179.0);
        assert_close(y, 89.0);
    }

    #[test]
    fn seam_wraps_around_center_azimuth() {
        let mapping = EquirectangularMapping::new(360, 180, 180.0);
        let left = mapping.pixel_center_to_alt_az(0, 89);
        let right = mapping.pixel_center_to_alt_az(359, 89);
        assert_close(left.azimuth_deg, 0.5);
        assert_close(right.azimuth_deg, 359.5);
    }
}
