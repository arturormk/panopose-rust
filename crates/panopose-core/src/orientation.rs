use glam::{DQuat, DVec3, EulerRot};
use serde::{Deserialize, Serialize};

use crate::coords::AltAz;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Orientation {
    pub w: f64,
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Default for Orientation {
    fn default() -> Self {
        Self::identity()
    }
}

impl Orientation {
    pub fn identity() -> Self {
        Self::from_quat(DQuat::IDENTITY)
    }

    pub fn from_quat(quat: DQuat) -> Self {
        let q = quat.normalize();
        Self {
            w: q.w,
            x: q.x,
            y: q.y,
            z: q.z,
        }
    }

    pub fn to_quat(self) -> DQuat {
        DQuat::from_xyzw(self.x, self.y, self.z, self.w).normalize()
    }

    pub fn is_valid(self) -> bool {
        self.w.is_finite()
            && self.x.is_finite()
            && self.y.is_finite()
            && self.z.is_finite()
            && (self.w * self.w + self.x * self.x + self.y * self.y + self.z * self.z) > 1e-20
    }

    pub fn normalized(self) -> Option<Self> {
        self.is_valid().then(|| Self::from_quat(self.to_quat()))
    }

    pub fn compose(self, rhs: Self) -> Self {
        Self::from_quat(self.to_quat() * rhs.to_quat())
    }

    pub fn to_legacy_ui_euler_deg(self, base_yaw_deg: f64) -> (f64, f64, f64) {
        let full = self.compose(Self::from_yaw_pitch_roll_deg(base_yaw_deg, 0.0, 0.0));
        let (yaw_with_base, roll, pitch) = full.to_quat().to_euler(EulerRot::YXZ);
        (
            (yaw_with_base.to_degrees() - base_yaw_deg + 180.0).rem_euclid(360.0) - 180.0,
            pitch.to_degrees(),
            roll.to_degrees(),
        )
    }

    pub fn from_yaw_pitch_roll_deg(yaw_deg: f64, pitch_deg: f64, roll_deg: f64) -> Self {
        let yaw = DQuat::from_axis_angle(DVec3::Y, yaw_deg.to_radians());
        let pitch = DQuat::from_axis_angle(DVec3::X, pitch_deg.to_radians());
        let roll = DQuat::from_axis_angle(DVec3::Z, roll_deg.to_radians());
        Self::from_quat(yaw * pitch * roll)
    }

    pub fn rotate_source_to_world(self, source_direction: DVec3) -> DVec3 {
        self.to_quat() * source_direction
    }

    pub fn rotate_world_to_source(self, world_direction: DVec3) -> DVec3 {
        self.to_quat().inverse() * world_direction
    }

    pub fn source_alt_az_to_world(self, source: AltAz) -> AltAz {
        AltAz::from_unit_vector(self.rotate_source_to_world(source.to_unit_vector()))
    }

    pub fn world_alt_az_to_source(self, world: AltAz) -> AltAz {
        AltAz::from_unit_vector(self.rotate_world_to_source(world.to_unit_vector()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_angle_close(a: f64, b: f64) {
        let delta = crate::coords::signed_degrees(a - b).abs();
        assert!(delta < 1e-9, "{a} != {b}");
    }

    #[test]
    fn identity_orientation_preserves_alt_az() {
        let o = Orientation::identity();
        let world = o.source_alt_az_to_world(AltAz {
            altitude_deg: 12.5,
            azimuth_deg: 247.25,
        });
        assert!((world.altitude_deg - 12.5).abs() < 1e-9);
        assert_angle_close(world.azimuth_deg, 247.25);
    }

    #[test]
    fn yaw_rotates_azimuth() {
        let o = Orientation::from_yaw_pitch_roll_deg(90.0, 0.0, 0.0);
        let world = o.source_alt_az_to_world(AltAz::NORTH_HORIZON);
        assert!((world.altitude_deg).abs() < 1e-9);
        assert_angle_close(world.azimuth_deg, 90.0);
    }

    #[test]
    fn inverse_recovers_source_direction() {
        let o = Orientation::from_yaw_pitch_roll_deg(17.0, -3.0, 1.25);
        let source = AltAz {
            altitude_deg: -12.0,
            azimuth_deg: 315.0,
        };
        let round_trip = o.world_alt_az_to_source(o.source_alt_az_to_world(source));
        assert!((round_trip.altitude_deg - source.altitude_deg).abs() < 1e-9);
        assert_angle_close(round_trip.azimuth_deg, source.azimuth_deg);
    }

    #[test]
    fn legacy_ui_euler_round_trips_a_composed_pose() {
        let source = Orientation::from_quat(
            DQuat::from_axis_angle(DVec3::Z, 0.17)
                * DQuat::from_axis_angle(DVec3::X, -0.23)
                * DQuat::from_axis_angle(DVec3::Y, 0.41),
        );
        let (yaw, pitch, roll) = source.to_legacy_ui_euler_deg(-90.0);
        let full = DQuat::from_euler(
            EulerRot::YXZ,
            (yaw - 90.0).to_radians(),
            roll.to_radians(),
            pitch.to_radians(),
        );
        let recovered =
            Orientation::from_quat(full * DQuat::from_axis_angle(DVec3::Y, 90_f64.to_radians()));
        assert!((1.0 - source.to_quat().dot(recovered.to_quat()).abs()) < 1e-12);
    }
}
