//! Pure orbital mechanics: TLE parsing, look angles, pass prediction and Doppler.

use std::fmt;

use chrono::{DateTime, Duration, Utc};

/// A satellite parsed from a TLE, ready to propagate.
pub struct Satellite {
    name: String,
    elements: sgp4::Elements,
    constants: sgp4::Constants,
}

impl Satellite {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn norad_id(&self) -> u64 {
        self.elements.norad_id
    }
}

#[derive(Debug)]
pub enum ParseError {
    /// A TLE line pair was malformed (bad checksum, wrong length, bad field...).
    Tle { name: String, reason: String },
    /// A line `1` had no matching line `2`.
    MissingLine2 { name: String },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Tle { name, reason } => write!(f, "bad TLE for {name:?}: {reason}"),
            ParseError::MissingLine2 { name } => write!(f, "TLE for {name:?} has no line 2"),
        }
    }
}

impl std::error::Error for ParseError {}

/// Parse CelesTrak-style TLE text: an optional name line, then lines `1` and `2`, repeated.
pub fn parse_tles(text: &str) -> Result<Vec<Satellite>, ParseError> {
    let mut sats = Vec::new();
    let mut name: Option<String> = None;
    let mut lines = text.lines().map(str::trim_end).filter(|l| !l.is_empty());
    while let Some(line) = lines.next() {
        if !line.starts_with("1 ") {
            name = Some(line.trim().to_string());
            continue;
        }
        let label = name
            .take()
            .unwrap_or_else(|| line.get(2..7).unwrap_or("?").to_string());
        let line2 = lines
            .next()
            .filter(|l| l.starts_with("2 "))
            .ok_or_else(|| ParseError::MissingLine2 {
                name: label.clone(),
            })?;
        let tle_err = |e: &dyn fmt::Display| ParseError::Tle {
            name: label.clone(),
            reason: e.to_string(),
        };
        let elements =
            sgp4::Elements::from_tle(Some(label.clone()), line.as_bytes(), line2.as_bytes())
                .map_err(|e| tle_err(&e))?;
        let constants = sgp4::Constants::from_elements(&elements).map_err(|e| tle_err(&e))?;
        sats.push(Satellite {
            name: label,
            elements,
            constants,
        });
    }
    Ok(sats)
}

/// Observer position on the WGS84 ellipsoid. Runtime input; nothing is baked in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Observer {
    pub lat_deg: f64,
    pub lon_deg: f64,
    /// Height above the ellipsoid in metres.
    pub alt_m: f64,
}

/// Where a satellite appears from an observer at one instant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LookAngle {
    /// Degrees clockwise from true north, in `[0, 360)`.
    pub azimuth_deg: f64,
    /// Degrees above the horizon (negative when below).
    pub elevation_deg: f64,
    pub range_km: f64,
    /// Positive when the satellite is receding.
    pub range_rate_km_s: f64,
}

#[derive(Debug)]
pub enum PropagationError {
    /// The time is too far from the TLE epoch to represent.
    OutOfRange,
    /// SGP4 diverged (typically a stale TLE).
    Sgp4(String),
}

impl fmt::Display for PropagationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PropagationError::OutOfRange => write!(f, "time out of range for this TLE"),
            PropagationError::Sgp4(e) => write!(f, "SGP4 propagation failed: {e}"),
        }
    }
}

impl std::error::Error for PropagationError {}

type Vec3 = [f64; 3];

/// WGS84 semi-major axis (km) and flattening.
const WGS84_A_KM: f64 = 6378.137;
const WGS84_F: f64 = 1.0 / 298.257_223_563;
/// Earth rotation rate in rad/s.
const EARTH_OMEGA: f64 = 7.292_115_0e-5;

fn dot(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// Rotate about +z by `angle` radians.
fn rot_z(v: Vec3, angle: f64) -> Vec3 {
    let (s, c) = angle.sin_cos();
    [c * v[0] - s * v[1], s * v[0] + c * v[1], v[2]]
}

/// Greenwich mean sidereal time in radians (IAU 1982), treating UTC as UT1.
fn gmst_rad(time: DateTime<Utc>) -> f64 {
    let jd = time.timestamp_micros() as f64 / 86_400e6 + 2_440_587.5;
    let t = (jd - 2_451_545.0) / 36_525.0;
    let deg = 280.460_618_37 + 360.985_647_366_29 * (jd - 2_451_545.0) + 0.000_387_933 * t * t
        - t * t * t / 38_710_000.0;
    deg.rem_euclid(360.0).to_radians()
}

/// Observer position in the Earth-fixed frame, km.
fn observer_ecef(o: &Observer) -> Vec3 {
    let (lat, lon) = (o.lat_deg.to_radians(), o.lon_deg.to_radians());
    let e2 = WGS84_F * (2.0 - WGS84_F);
    let n = WGS84_A_KM / (1.0 - e2 * lat.sin().powi(2)).sqrt();
    let h = o.alt_m / 1000.0;
    [
        (n + h) * lat.cos() * lon.cos(),
        (n + h) * lat.cos() * lon.sin(),
        (n * (1.0 - e2) + h) * lat.sin(),
    ]
}

impl Satellite {
    /// Azimuth, elevation, range and range rate of this satellite as seen by `observer`.
    pub fn look_angle(
        &self,
        observer: &Observer,
        time: DateTime<Utc>,
    ) -> Result<LookAngle, PropagationError> {
        let minutes = self
            .elements
            .datetime_to_minutes_since_epoch(&time.naive_utc())
            .map_err(|_| PropagationError::OutOfRange)?;
        let prediction = self
            .constants
            .propagate(minutes)
            .map_err(|e| PropagationError::Sgp4(format!("{e:?}")))?;

        // Work in TEME: rotate the fixed observer into it by the sidereal angle.
        let gmst = gmst_rad(time);
        let obs_teme = rot_z(observer_ecef(observer), gmst);
        let obs_vel = [-EARTH_OMEGA * obs_teme[1], EARTH_OMEGA * obs_teme[0], 0.0];
        let rel_pos = sub(prediction.position, obs_teme);
        let rel_vel = sub(prediction.velocity, obs_vel);
        let range = dot(rel_pos, rel_pos).sqrt();

        // Local east/north/up at the observer, from the Earth-fixed relative vector.
        let rel_ecef = rot_z(rel_pos, -gmst);
        let (lat, lon) = (observer.lat_deg.to_radians(), observer.lon_deg.to_radians());
        let east = [-lon.sin(), lon.cos(), 0.0];
        let north = [-lat.sin() * lon.cos(), -lat.sin() * lon.sin(), lat.cos()];
        let up = [lat.cos() * lon.cos(), lat.cos() * lon.sin(), lat.sin()];
        let (e, n, u) = (dot(rel_ecef, east), dot(rel_ecef, north), dot(rel_ecef, up));

        Ok(LookAngle {
            azimuth_deg: e.atan2(n).to_degrees().rem_euclid(360.0),
            elevation_deg: (u / range).asin().to_degrees(),
            range_km: range,
            range_rate_km_s: dot(rel_pos, rel_vel) / range,
        })
    }
}

/// One visible pass of a satellite above a minimum elevation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pass {
    /// Acquisition of signal. Clipped to the search start if the pass was already under way.
    pub aos: DateTime<Utc>,
    /// Time of closest approach, i.e. maximum elevation.
    pub tca: DateTime<Utc>,
    /// Loss of signal. Clipped to the search end if the pass is still under way.
    pub los: DateTime<Utc>,
    pub max_elevation_deg: f64,
    pub aos_azimuth_deg: f64,
    pub los_azimuth_deg: f64,
}

/// Coarse scan step. Passes above `min_elevation_deg` shorter than this can be missed.
const SCAN_STEP: Duration = Duration::seconds(20);
/// Crossing and peak times are refined to this resolution.
const TIME_RESOLUTION_MS: i64 = 10;

impl Satellite {
    /// Passes whose elevation reaches `min_elevation_deg` between `from` and `to`, in time order.
    pub fn passes(
        &self,
        observer: &Observer,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        min_elevation_deg: f64,
    ) -> Result<Vec<Pass>, PropagationError> {
        let look = |t| self.look_angle(observer, t);
        let above = |t| look(t).map(|l| l.elevation_deg >= min_elevation_deg);

        let mut passes = Vec::new();
        let mut aos = above(from)?.then_some(from);
        let mut prev = from;
        while prev < to {
            let cur = (prev + SCAN_STEP).min(to);
            let is_above = above(cur)?;
            match (aos, is_above) {
                (None, true) => {
                    aos = Some(self.crossing(observer, prev, cur, min_elevation_deg)?)
                }
                (Some(start), false) => {
                    let los = self.crossing(observer, prev, cur, min_elevation_deg)?;
                    passes.push(self.build_pass(observer, start, los)?);
                    aos = None;
                }
                _ => {}
            }
            prev = cur;
        }
        if let Some(start) = aos {
            passes.push(self.build_pass(observer, start, to)?);
        }
        Ok(passes)
    }

    /// Bisect for the instant elevation crosses `min_el` between `a` and `b`.
    fn crossing(
        &self,
        observer: &Observer,
        mut a: DateTime<Utc>,
        mut b: DateTime<Utc>,
        min_el: f64,
    ) -> Result<DateTime<Utc>, PropagationError> {
        let a_above = self.look_angle(observer, a)?.elevation_deg >= min_el;
        while (b - a).num_milliseconds() > TIME_RESOLUTION_MS {
            let mid = a + (b - a) / 2;
            if (self.look_angle(observer, mid)?.elevation_deg >= min_el) == a_above {
                a = mid;
            } else {
                b = mid;
            }
        }
        Ok(b)
    }

    fn build_pass(
        &self,
        observer: &Observer,
        aos: DateTime<Utc>,
        los: DateTime<Utc>,
    ) -> Result<Pass, PropagationError> {
        // Elevation rises then falls within a pass, so ternary search finds the peak.
        let (mut lo, mut hi) = (aos, los);
        while (hi - lo).num_milliseconds() > TIME_RESOLUTION_MS {
            let third = (hi - lo) / 3;
            let (m1, m2) = (lo + third, hi - third);
            if self.look_angle(observer, m1)?.elevation_deg
                < self.look_angle(observer, m2)?.elevation_deg
            {
                lo = m1;
            } else {
                hi = m2;
            }
        }
        let tca = lo + (hi - lo) / 2;
        Ok(Pass {
            aos,
            tca,
            los,
            max_elevation_deg: self.look_angle(observer, tca)?.elevation_deg,
            aos_azimuth_deg: self.look_angle(observer, aos)?.azimuth_deg,
            los_azimuth_deg: self.look_angle(observer, los)?.azimuth_deg,
        })
    }
}

/// Speed of light in km/s.
const C_KM_S: f64 = 299_792.458;

/// Frequency a ground station hears for a satellite transmitting `nominal_hz`.
/// `range_rate_km_s` is positive when the satellite is receding.
pub fn downlink_hz(nominal_hz: f64, range_rate_km_s: f64) -> f64 {
    nominal_hz * (1.0 - range_rate_km_s / C_KM_S)
}

/// Frequency to transmit so the satellite receives `nominal_hz`.
/// `range_rate_km_s` is positive when the satellite is receding.
pub fn uplink_hz(nominal_hz: f64, range_rate_km_s: f64) -> f64 {
    nominal_hz * (1.0 + range_rate_km_s / C_KM_S)
}
