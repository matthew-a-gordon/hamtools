//! Where the operator is: coordinates or a Maidenhead grid, from the command line, the config
//! file or a town-level default.

use std::fmt;

use logbook::Grid;
use orbit::Observer;

/// Town-level Middlebury, IN (never a street address).
const DEFAULT: Observer = Observer {
    lat_deg: 41.67,
    lon_deg: -85.71,
    alt_m: 250.0,
};

/// One source of position input; every field is optional.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Spec {
    /// Maidenhead locator, 4 or 6 characters.
    pub grid: Option<String>,
    pub lat_deg: Option<f64>,
    pub lon_deg: Option<f64>,
    pub alt_m: Option<f64>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ObserverError(String);

impl fmt::Display for ObserverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "observer: {}", self.0)
    }
}

impl std::error::Error for ObserverError {}

fn err<T>(msg: impl Into<String>) -> Result<T, ObserverError> {
    Err(ObserverError(msg.into()))
}

/// Latitude and longitude of the centre of a Maidenhead square (4 characters) or subsquare (6).
pub fn grid_centre(grid: &str) -> Result<(f64, f64), ObserverError> {
    let chars: Vec<char> = grid.to_ascii_uppercase().chars().collect();
    if chars.len() != 4 && chars.len() != 6 {
        return err(format!("grid {grid:?} must be 4 or 6 characters"));
    }
    let letter = |c: char, last: char| (c >= 'A' && c <= last).then(|| f64::from(c as u8 - b'A'));
    let digit = |c: char| c.to_digit(10).map(f64::from);
    let (Some(field_lon), Some(field_lat), Some(sq_lon), Some(sq_lat)) = (
        letter(chars[0], 'R'),
        letter(chars[1], 'R'),
        digit(chars[2]),
        digit(chars[3]),
    ) else {
        return err(format!("grid {grid:?} is not a valid locator"));
    };
    let mut lon = -180.0 + 20.0 * field_lon + 2.0 * sq_lon;
    let mut lat = -90.0 + 10.0 * field_lat + sq_lat;
    if chars.len() == 6 {
        let (Some(sub_lon), Some(sub_lat)) = (letter(chars[4], 'X'), letter(chars[5], 'X')) else {
            return err(format!("grid {grid:?} has an invalid subsquare"));
        };
        lon += sub_lon / 12.0 + 1.0 / 24.0;
        lat += sub_lat / 24.0 + 1.0 / 48.0;
    } else {
        lon += 1.0;
        lat += 0.5;
    }
    Ok((lat, lon))
}

impl Spec {
    fn position(&self) -> Result<Option<(f64, f64)>, ObserverError> {
        match (&self.grid, self.lat_deg, self.lon_deg) {
            (Some(_), Some(_), _) | (Some(_), _, Some(_)) => {
                err("give either a grid or lat/lon, not both")
            }
            (Some(grid), None, None) => grid_centre(grid).map(Some),
            (None, Some(lat), Some(lon)) => {
                if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
                    return err(format!("coordinates {lat}, {lon} are out of range"));
                }
                Ok(Some((lat, lon)))
            }
            (None, Some(_), None) | (None, None, Some(_)) => {
                err("lat and lon must be given together")
            }
            (None, None, None) => Ok(None),
        }
    }
}

/// Choose the observer: position from the command line, else sat-tui's own `[observer]`, else
/// the shared config's Locator, else the default; altitude chosen from the first two
/// independently of position.
pub fn resolve(
    cli: &Spec,
    config: &Spec,
    shared_locator: Option<&Grid>,
) -> Result<Observer, ObserverError> {
    let shared = shared_locator
        .map(|locator| grid_centre(locator.as_str()))
        .transpose()?;
    let (lat_deg, lon_deg) = match cli.position()?.or(config.position()?).or(shared) {
        Some(position) => position,
        None => (DEFAULT.lat_deg, DEFAULT.lon_deg),
    };
    let alt_m = cli.alt_m.or(config.alt_m).unwrap_or(DEFAULT.alt_m);
    Ok(Observer {
        lat_deg,
        lon_deg,
        alt_m,
    })
}
