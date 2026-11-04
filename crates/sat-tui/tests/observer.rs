//! Maidenhead expectations are worked by hand from the locator definition (field = 20 deg x 10 deg,
//! square = 2 x 1, subsquare = 5' x 2.5'), taking the centre of the cell:
//!   EN71   : lon -180 + 4*20 + 7*2 + 1 = -85.0 ; lat -90 + 13*10 + 1*1 + 0.5 = 41.5
//!   EN71dq : lon -86 + 3/12 + 1/24 = -85.70833 ; lat 41 + 16/24 + 1/48 = 41.68750
//!            (first subsquare letter is longitude: d = 3; second is latitude: q = 16)

use orbit::Observer;
use sat_tui::observer::{Spec, grid_centre, resolve};

fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-4, "{a} vs {b}");
}

#[test]
fn a_four_character_grid_resolves_to_the_square_centre() {
    let (lat, lon) = grid_centre("EN71").unwrap();
    close(lat, 41.5);
    close(lon, -85.0);
}

#[test]
fn a_six_character_grid_resolves_to_the_subsquare_centre_in_any_case() {
    let (lat, lon) = grid_centre("en71DQ").unwrap();
    close(lat, 41.6875);
    close(lon, -85.708_333);
}

#[test]
fn rejects_malformed_grids() {
    for bad in [
        "", "E", "EN7", "EN71q", "ZZ99", "EN71ya", "EN71qdx", "1N71", "EN7a",
    ] {
        assert!(grid_centre(bad).is_err(), "{bad:?} should be rejected");
    }
}

fn spec(grid: Option<&str>, lat: Option<f64>, lon: Option<f64>, alt: Option<f64>) -> Spec {
    Spec {
        grid: grid.map(String::from),
        lat_deg: lat,
        lon_deg: lon,
        alt_m: alt,
    }
}

const NOTHING: Spec = Spec {
    grid: None,
    lat_deg: None,
    lon_deg: None,
    alt_m: None,
};

#[test]
fn with_nothing_configured_it_falls_back_to_town_level_middlebury() {
    let o = resolve(&NOTHING, &NOTHING, None).unwrap();

    assert_eq!(
        o,
        Observer {
            lat_deg: 41.67,
            lon_deg: -85.71,
            alt_m: 250.0
        }
    );
}

#[test]
fn config_values_replace_the_defaults() {
    let config = spec(None, Some(40.0), Some(-105.0), Some(1600.0));

    let o = resolve(&NOTHING, &config, None).unwrap();

    assert_eq!(
        o,
        Observer {
            lat_deg: 40.0,
            lon_deg: -105.0,
            alt_m: 1600.0
        }
    );
}

#[test]
fn the_command_line_wins_over_the_config() {
    let cli = spec(None, Some(10.0), Some(20.0), None);
    let config = spec(None, Some(40.0), Some(-105.0), Some(1600.0));

    let o = resolve(&cli, &config, None).unwrap();

    // Position comes from the command line; altitude, unset there, from the config.
    assert_eq!(
        o,
        Observer {
            lat_deg: 10.0,
            lon_deg: 20.0,
            alt_m: 1600.0
        }
    );
}

#[test]
fn a_command_line_grid_beats_config_coordinates() {
    let cli = spec(Some("EN71"), None, None, None);
    let config = spec(None, Some(40.0), Some(-105.0), None);

    let o = resolve(&cli, &config, None).unwrap();

    close(o.lat_deg, 41.5);
    close(o.lon_deg, -85.0);
    assert_eq!(o.alt_m, 250.0);
}

#[test]
fn half_a_coordinate_pair_is_an_error() {
    assert!(resolve(&spec(None, Some(41.0), None, None), &NOTHING, None).is_err());
    assert!(resolve(&NOTHING, &spec(None, None, Some(-85.0), None), None).is_err());
}

#[test]
fn one_source_may_not_give_both_a_grid_and_coordinates() {
    assert!(
        resolve(
            &spec(Some("EN71"), Some(41.0), Some(-85.0), None),
            &NOTHING,
            None
        )
        .is_err()
    );
}

#[test]
fn out_of_range_coordinates_are_rejected() {
    assert!(resolve(&spec(None, Some(91.0), Some(0.0), None), &NOTHING, None).is_err());
    assert!(resolve(&spec(None, Some(0.0), Some(181.0), None), &NOTHING, None).is_err());
}

mod shared_locator {
    use super::*;
    use logbook::Grid;

    #[test]
    fn it_places_the_observer_when_no_flag_or_observer_section_does() {
        let locator = Grid::new("en71dq").unwrap();

        let o = resolve(&NOTHING, &NOTHING, Some(&locator)).unwrap();

        close(o.lat_deg, 41.6875);
        close(o.lon_deg, -85.70833);
        assert_eq!(o.alt_m, 250.0);
    }

    #[test]
    fn flags_and_the_observer_section_both_beat_it() {
        let locator = Grid::new("EN71DQ").unwrap();

        let o = resolve(
            &spec(None, Some(10.0), Some(20.0), None),
            &NOTHING,
            Some(&locator),
        )
        .unwrap();
        assert_eq!((o.lat_deg, o.lon_deg), (10.0, 20.0));

        let own = spec(Some("EN71"), None, None, None);
        let o = resolve(&NOTHING, &own, Some(&locator)).unwrap();
        close(o.lat_deg, 41.5);
        close(o.lon_deg, -85.0);
    }
}
