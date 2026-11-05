//! Positions on the globe: Maidenhead centres and great-circle distance, in one place.

/// A position as (latitude, longitude) in degrees.
pub type LatLon = (f64, f64);

/// The centre of a 4 or 6 character Maidenhead Locator.
pub fn grid_center(grid: &str) -> Option<LatLon> {
    let g: Vec<char> = grid.trim().to_ascii_uppercase().chars().collect();
    if g.len() != 4 && g.len() != 6 {
        return None;
    }
    let field = |c: char| ('A'..='R').contains(&c).then(|| f64::from(c as u8 - b'A'));
    let digit = |c: char| c.to_digit(10).map(f64::from);
    let sub = |c: char| ('A'..='X').contains(&c).then(|| f64::from(c as u8 - b'A'));
    let mut lon = -180.0 + 20.0 * field(g[0])? + 2.0 * digit(g[2])?;
    let mut lat = -90.0 + 10.0 * field(g[1])? + digit(g[3])?;
    if g.len() == 6 {
        lon += sub(g[4])? / 12.0 + 1.0 / 24.0;
        lat += sub(g[5])? / 24.0 + 1.0 / 48.0;
    } else {
        lon += 1.0;
        lat += 0.5;
    }
    Some((lat, lon))
}

/// Great-circle distance in kilometres.
pub fn distance_km(from: LatLon, to: LatLon) -> f64 {
    let (la1, lo1, la2, lo2) = (
        from.0.to_radians(),
        from.1.to_radians(),
        to.0.to_radians(),
        to.1.to_radians(),
    );
    let a = ((la2 - la1) / 2.0).sin().powi(2)
        + la1.cos() * la2.cos() * ((lo2 - lo1) / 2.0).sin().powi(2);
    6371.0 * 2.0 * a.sqrt().asin()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_grid_centre_of_middlebury() {
        let (lat, lon) = grid_center("EN71DQ").unwrap();
        assert!((lat - 41.6875).abs() < 0.001, "{lat}");
        assert!((lon + 85.7083).abs() < 0.001, "{lon}");
    }

    #[test]
    fn a_four_character_grid_is_its_square_centre() {
        assert_eq!(grid_center("EN71"), Some((41.5, -85.0)));
        assert_eq!(grid_center("EN7"), None);
        assert_eq!(grid_center("ZZ99"), None);
    }

    #[test]
    fn one_degree_of_longitude_at_41_7_north_is_83_km() {
        let d = distance_km((41.6875, -85.7083), (41.6875, -86.7083));
        assert!((82.0..84.5).contains(&d), "{d}");
        assert!(distance_km((10.0, 10.0), (10.0, 10.0)) < 0.001);
    }
}
