use astronomy_engine_bindings::{
    Astronomy_Equator, Astronomy_Horizon, Astronomy_MakeObserver, Astronomy_TimeFromUtc,
    astro_aberration_t_ABERRATION, astro_body_t, astro_body_t_BODY_JUPITER, astro_body_t_BODY_MARS,
    astro_body_t_BODY_MERCURY, astro_body_t_BODY_MOON, astro_body_t_BODY_SATURN,
    astro_body_t_BODY_SUN, astro_body_t_BODY_VENUS, astro_equator_date_t_EQUATOR_OF_DATE,
    astro_refraction_t_REFRACTION_NORMAL, astro_status_t_ASTRO_SUCCESS, astro_utc_t,
};
use chrono::{DateTime, Datelike, FixedOffset, Timelike};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

use crate::coords::AltAz;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CelestialObject {
    Sun,
    Moon,
    Mercury,
    Venus,
    Mars,
    Jupiter,
    Saturn,
    Sirius,
    Canopus,
    Arcturus,
    Vega,
    Capella,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CelestialMarker {
    pub object: CelestialObject,
    pub label: String,
    pub alt_az: AltAz,
    pub magnitude: Option<f64>,
    pub accuracy_note: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StarCatalogEntry {
    pub hr: u16,
    pub ra_hours: f64,
    pub dec_deg: f64,
    pub magnitude: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StarMarker {
    pub hr: u16,
    pub alt_az: AltAz,
    pub magnitude: f64,
}

pub trait AstronomyProvider {
    fn markers(
        &self,
        observer: Observer,
        time: DateTime<FixedOffset>,
        objects: &[CelestialObject],
    ) -> Vec<CelestialMarker>;
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Observer {
    pub latitude_deg: f64,
    pub longitude_deg: f64,
    pub elevation_m: f64,
}

#[derive(Debug, Default)]
pub struct ApproximateAstronomyProvider;

impl AstronomyProvider for ApproximateAstronomyProvider {
    fn markers(
        &self,
        observer: Observer,
        time: DateTime<FixedOffset>,
        objects: &[CelestialObject],
    ) -> Vec<CelestialMarker> {
        objects
            .iter()
            .filter_map(|object| marker_for(*object, observer, time))
            .collect()
    }
}

impl ApproximateAstronomyProvider {
    pub fn star_markers(&self, observer: Observer, time: DateTime<FixedOffset>) -> Vec<StarMarker> {
        bright_star_catalog()
            .iter()
            .filter_map(|star| {
                let alt_az = equatorial_to_horizontal(star.ra_hours, star.dec_deg, observer, time);
                (alt_az.altitude_deg >= 0.0).then_some(StarMarker {
                    hr: star.hr,
                    alt_az,
                    magnitude: star.magnitude,
                })
            })
            .collect()
    }
}

fn marker_for(
    object: CelestialObject,
    observer: Observer,
    time: DateTime<FixedOffset>,
) -> Option<CelestialMarker> {
    let (alt_az, magnitude, accuracy_note) = match astronomy_engine_body(object) {
        Some(body) => (
            astronomy_engine_horizontal(body, observer, time)?,
            approximate_magnitude(object),
            "Astronomy Engine topocentric apparent Alt/Az".into(),
        ),
        None => {
            let (ra_hours, dec_deg, magnitude) = fixed_star_equatorial(object)?;
            (
                equatorial_to_horizontal(ra_hours, dec_deg, observer, time),
                magnitude,
                "fixed bright-star RA/Dec approximation".into(),
            )
        }
    };

    Some(CelestialMarker {
        object,
        label: format!("{object:?}"),
        alt_az,
        magnitude,
        accuracy_note,
    })
}

fn astronomy_engine_horizontal(
    body: astro_body_t,
    observer: Observer,
    time: DateTime<FixedOffset>,
) -> Option<AltAz> {
    let utc = time.to_utc();
    let astro_utc = astro_utc_t {
        year: utc.year(),
        month: utc.month() as i32,
        day: utc.day() as i32,
        hour: utc.hour() as i32,
        minute: utc.minute() as i32,
        second: utc.second() as f64 + utc.nanosecond() as f64 / 1_000_000_000.0,
    };

    // Astronomy Engine uses the same horizontal convention as PanoPose:
    // azimuth 0=N, 90=E, altitude positive above the observer horizon.
    let mut astro_time = unsafe { Astronomy_TimeFromUtc(astro_utc) };
    let astro_observer = unsafe {
        Astronomy_MakeObserver(
            observer.latitude_deg,
            observer.longitude_deg,
            observer.elevation_m,
        )
    };
    let equatorial = unsafe {
        Astronomy_Equator(
            body,
            &mut astro_time,
            astro_observer,
            astro_equator_date_t_EQUATOR_OF_DATE,
            astro_aberration_t_ABERRATION,
        )
    };
    if equatorial.status != astro_status_t_ASTRO_SUCCESS {
        return None;
    }

    let horizon = unsafe {
        Astronomy_Horizon(
            &mut astro_time,
            astro_observer,
            equatorial.ra,
            equatorial.dec,
            astro_refraction_t_REFRACTION_NORMAL,
        )
    };

    Some(AltAz {
        altitude_deg: horizon.altitude,
        azimuth_deg: horizon.azimuth,
    })
}

fn astronomy_engine_body(object: CelestialObject) -> Option<astro_body_t> {
    match object {
        CelestialObject::Sun => Some(astro_body_t_BODY_SUN),
        CelestialObject::Moon => Some(astro_body_t_BODY_MOON),
        CelestialObject::Mercury => Some(astro_body_t_BODY_MERCURY),
        CelestialObject::Venus => Some(astro_body_t_BODY_VENUS),
        CelestialObject::Mars => Some(astro_body_t_BODY_MARS),
        CelestialObject::Jupiter => Some(astro_body_t_BODY_JUPITER),
        CelestialObject::Saturn => Some(astro_body_t_BODY_SATURN),
        CelestialObject::Sirius
        | CelestialObject::Canopus
        | CelestialObject::Arcturus
        | CelestialObject::Vega
        | CelestialObject::Capella => None,
    }
}

fn approximate_magnitude(object: CelestialObject) -> Option<f64> {
    Some(match object {
        CelestialObject::Sun => -26.74,
        CelestialObject::Moon => -12.0,
        CelestialObject::Mercury => -0.4,
        CelestialObject::Venus => -4.0,
        CelestialObject::Mars => -1.0,
        CelestialObject::Jupiter => -2.2,
        CelestialObject::Saturn => 0.7,
        CelestialObject::Sirius => -1.46,
        CelestialObject::Canopus => -0.74,
        CelestialObject::Arcturus => -0.05,
        CelestialObject::Vega => 0.03,
        CelestialObject::Capella => 0.08,
    })
}

fn fixed_star_equatorial(object: CelestialObject) -> Option<(f64, f64, Option<f64>)> {
    Some(match object {
        CelestialObject::Sirius => (6.7525, -16.7161, Some(-1.46)),
        CelestialObject::Canopus => (6.3992, -52.6957, Some(-0.74)),
        CelestialObject::Arcturus => (14.2610, 19.1825, Some(-0.05)),
        CelestialObject::Vega => (18.6156, 38.7837, Some(0.03)),
        CelestialObject::Capella => (5.2782, 45.9980, Some(0.08)),
        CelestialObject::Sun
        | CelestialObject::Moon
        | CelestialObject::Mercury
        | CelestialObject::Venus
        | CelestialObject::Mars
        | CelestialObject::Jupiter
        | CelestialObject::Saturn => return None,
    })
}

fn equatorial_to_horizontal(
    ra_hours: f64,
    dec_deg: f64,
    observer: Observer,
    time: DateTime<FixedOffset>,
) -> AltAz {
    let lst_deg = local_sidereal_time_deg(time, observer.longitude_deg);
    let hour_angle = (lst_deg - ra_hours * 15.0).rem_euclid(360.0).to_radians();
    let dec = dec_deg.to_radians();
    let lat = observer.latitude_deg.to_radians();
    let sin_alt = dec.sin() * lat.sin() + dec.cos() * lat.cos() * hour_angle.cos();
    let altitude = sin_alt.clamp(-1.0, 1.0).asin();
    let az = (-hour_angle.sin()).atan2(dec.tan() * lat.cos() - lat.sin() * hour_angle.cos());
    AltAz {
        altitude_deg: altitude.to_degrees(),
        azimuth_deg: az.to_degrees().rem_euclid(360.0),
    }
}

fn local_sidereal_time_deg(time: DateTime<FixedOffset>, longitude_deg: f64) -> f64 {
    let jd = julian_day(time);
    let d = jd - 2_451_545.0;
    (280.460_618_37 + 360.985_647_366_29 * d + longitude_deg).rem_euclid(360.0)
}

fn julian_day(time: DateTime<FixedOffset>) -> f64 {
    let utc = time.to_utc();
    let mut year = utc.year();
    let mut month = utc.month() as i32;
    let day = utc.day() as i32;
    if month <= 2 {
        year -= 1;
        month += 12;
    }
    let a = (year as f64 / 100.0).floor();
    let b = 2.0 - a + (a / 4.0).floor();
    let day_fraction = (utc.hour() as f64
        + utc.minute() as f64 / 60.0
        + (utc.second() as f64 + utc.nanosecond() as f64 / 1_000_000_000.0) / 3600.0)
        / 24.0;
    (365.25 * (year + 4716) as f64).floor()
        + (30.6001 * (month + 1) as f64).floor()
        + day as f64
        + day_fraction
        + b
        - 1524.5
}

pub fn bright_star_catalog() -> &'static [StarCatalogEntry] {
    static CATALOG: OnceLock<Vec<StarCatalogEntry>> = OnceLock::new();
    CATALOG.get_or_init(parse_bright_star_catalog)
}

fn parse_bright_star_catalog() -> Vec<StarCatalogEntry> {
    include_str!("../data/bright_stars_1500.csv")
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(parse_star_catalog_line)
        .collect()
}

fn parse_star_catalog_line(line: &str) -> StarCatalogEntry {
    let mut fields = line.split(',');
    let hr = fields
        .next()
        .and_then(|value| value.parse().ok())
        .expect("embedded star catalog row has invalid HR id");
    let ra_hours = fields
        .next()
        .and_then(|value| value.parse().ok())
        .expect("embedded star catalog row has invalid right ascension");
    let dec_deg = fields
        .next()
        .and_then(|value| value.parse().ok())
        .expect("embedded star catalog row has invalid declination");
    let magnitude = fields
        .next()
        .and_then(|value| value.parse().ok())
        .expect("embedded star catalog row has invalid magnitude");
    assert!(
        fields.next().is_none(),
        "embedded star catalog row has extra fields"
    );
    StarCatalogEntry {
        hr,
        ra_hours,
        dec_deg,
        magnitude,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn angular_separation(a: AltAz, b: AltAz) -> f64 {
        a.to_unit_vector()
            .dot(b.to_unit_vector())
            .clamp(-1.0, 1.0)
            .acos()
            .to_degrees()
    }

    #[test]
    fn astronomy_provider_returns_requested_markers() {
        let provider = ApproximateAstronomyProvider;
        let markers = provider.markers(
            Observer {
                latitude_deg: 40.0,
                longitude_deg: -3.0,
                elevation_m: 700.0,
            },
            "2026-08-29T18:37:00+02:00".parse().unwrap(),
            &[
                CelestialObject::Sun,
                CelestialObject::Moon,
                CelestialObject::Vega,
            ],
        );
        assert_eq!(markers.len(), 3);
        assert!(markers.iter().all(
            |marker| marker.alt_az.altitude_deg >= -90.0 && marker.alt_az.altitude_deg <= 90.0
        ));
    }

    #[test]
    fn moon_is_far_from_sun_near_late_august_2026_full_moon() {
        let provider = ApproximateAstronomyProvider;
        let markers = provider.markers(
            Observer {
                latitude_deg: 40.4168,
                longitude_deg: -3.7038,
                elevation_m: 667.0,
            },
            "2026-08-29T22:00:00+02:00".parse().unwrap(),
            &[CelestialObject::Sun, CelestialObject::Moon],
        );
        let sun = markers
            .iter()
            .find(|marker| marker.object == CelestialObject::Sun)
            .unwrap();
        let moon = markers
            .iter()
            .find(|marker| marker.object == CelestialObject::Moon)
            .unwrap();
        assert!(angular_separation(sun.alt_az, moon.alt_az) > 120.0);
    }

    #[test]
    fn bright_star_catalog_is_limited_and_sorted_by_magnitude() {
        let catalog = bright_star_catalog();
        assert_eq!(catalog.len(), 1500);
        assert_eq!(catalog[0].hr, 2491);
        assert!(
            catalog
                .windows(2)
                .all(|pair| pair[0].magnitude <= pair[1].magnitude)
        );
    }

    #[test]
    fn star_markers_return_only_above_horizon_stars() {
        let provider = ApproximateAstronomyProvider;
        let markers = provider.star_markers(
            Observer {
                latitude_deg: 40.4168,
                longitude_deg: -3.7038,
                elevation_m: 667.0,
            },
            "2026-08-29T22:00:00+02:00".parse().unwrap(),
        );
        assert!(!markers.is_empty());
        assert!(markers.len() < bright_star_catalog().len());
        assert!(
            markers
                .iter()
                .all(|marker| marker.alt_az.altitude_deg >= 0.0)
        );
    }
}
