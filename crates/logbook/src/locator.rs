//! Maidenhead locators: a validated [`Grid`], from text or from a position.

use std::fmt;

/// 6-character Maidenhead locator (uppercase) for a position in degrees.
fn locator(lat_deg: f64, lon_deg: f64) -> String {
    let lon = (lon_deg + 180.0).clamp(0.0, 360.0 - 1e-9);
    let lat = (lat_deg + 90.0).clamp(0.0, 180.0 - 1e-9);
    let idx = |v: f64, max: usize| (v.floor() as usize).min(max);
    let letter = |i: usize| char::from(b'A' + i as u8);
    let digit = |i: usize| char::from(b'0' + i as u8);
    let (lon_field, lat_field) = (idx(lon / 20.0, 17), idx(lat / 10.0, 17));
    let (lon_rem, lat_rem) = (lon - lon_field as f64 * 20.0, lat - lat_field as f64 * 10.0);
    let (lon_sq, lat_sq) = (idx(lon_rem / 2.0, 9), idx(lat_rem, 9));
    let (lon_rem, lat_rem) = (lon_rem - lon_sq as f64 * 2.0, lat_rem - lat_sq as f64);
    let (lon_sub, lat_sub) = (idx(lon_rem * 12.0, 23), idx(lat_rem * 24.0, 23));
    [
        letter(lon_field),
        letter(lat_field),
        digit(lon_sq),
        digit(lat_sq),
        letter(lon_sub),
        letter(lat_sub),
    ]
    .iter()
    .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocatorError(pub String);

impl fmt::Display for LocatorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid grid {:?}: expected 6 characters like EN71DQ",
            self.0
        )
    }
}

impl std::error::Error for LocatorError {}

/// A valid 6-character Maidenhead locator, stored uppercase.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grid(String);

impl Grid {
    /// Validate a 6-character locator (case-insensitive).
    pub fn new(raw: &str) -> Result<Grid, LocatorError> {
        let up = raw.trim().to_ascii_uppercase();
        let b = up.as_bytes();
        let ok = b.len() == 6
            && b[0..2].iter().all(|c| (b'A'..=b'R').contains(c))
            && b[2..4].iter().all(u8::is_ascii_digit)
            && b[4..6].iter().all(|c| (b'A'..=b'X').contains(c));
        if ok {
            Ok(Grid(up))
        } else {
            Err(LocatorError(raw.to_string()))
        }
    }

    /// The locator for a position in degrees.
    pub fn at(lat_deg: f64, lon_deg: f64) -> Grid {
        Grid(locator(lat_deg, lon_deg))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Grid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn middlebury_in() {
        assert_eq!(locator(41.67, -85.71), "EN71DQ");
    }

    #[test]
    fn greenwich() {
        assert_eq!(locator(51.4779, -0.0015), "IO91XL");
    }

    #[test]
    fn extremes_stay_valid() {
        assert_eq!(locator(-90.0, -180.0), "AA00AA");
        assert_eq!(locator(90.0, 180.0), "RR99XX");
    }

    #[test]
    fn grid_validation() {
        assert_eq!(Grid::new("en71dq").unwrap().as_str(), "EN71DQ");
        for bad in [
            "EN71", "EN71DQ11", "SN71DQ", "EN7ADQ", "EN71DY", "EN71D", "",
        ] {
            assert!(Grid::new(bad).is_err(), "{bad}");
        }
    }
}
