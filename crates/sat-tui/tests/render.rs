//! Rendering is checked through ratatui's TestBackend. Look angles and Doppler are Skyfield's
//! for the ISS at 2026-10-26 13:15:00Z: az 299.496, el 16.344, range 1183.5 km, 6.557 km/s
//! approaching, so 145.825 MHz is heard at 145,828,190 Hz (worked with awk, c = 299792.458 km/s).

use chrono::{DateTime, Duration, TimeZone, Utc};
use orbit::{Observer, parse_tles};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use sat_tui::app::{App, Settings};
use sat_tui::satnogs::parse_transmitters;
use sat_tui::ui::{Footer, render};

fn t(h: u32, m: u32, s: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 26, h, m, s).unwrap()
}

fn app() -> App {
    App::new(
        parse_tles(include_str!("fixtures/sample.tle")).unwrap(),
        parse_transmitters(include_str!("fixtures/satnogs_transmitters.json")).unwrap(),
        Observer {
            lat_deg: 41.67,
            lon_deg: -85.71,
            alt_m: 250.0,
        },
        Settings {
            watch: vec![],
            horizon_hours: 24,
            min_elevation_deg: 10.0,
        },
        t(12, 0, 0),
    )
}

fn footer(now: DateTime<Utc>) -> Footer {
    Footer {
        now,
        tle_age: Some(Duration::hours(3)),
        transmitter_age: Some(Duration::hours(5)),
        tles_stale: false,
        rig: "not connected".to_string(),
    }
}

fn screen(app: &App, footer: &Footer) -> Vec<String> {
    let (width, height) = (110u16, 32u16);
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| render(app, footer, frame)).unwrap();
    terminal
        .backend()
        .buffer()
        .content()
        .chunks(width as usize)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect())
        .collect()
}

fn text(rows: &[String]) -> String {
    rows.join("\n")
}

fn press(app: &mut App, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}

#[test]
fn the_pass_list_shows_each_pass_with_its_start_time_and_peak_elevation() {
    let screen = text(&screen(&app(), &footer(t(13, 15, 0))));

    assert!(screen.contains("ISS (ZARYA)"), "{screen}");
    assert!(screen.contains("13:14"), "{screen}");
    assert!(screen.contains("70°"), "{screen}");
    assert!(screen.contains("RS-44"), "{screen}");
}

#[test]
fn the_live_panel_shows_where_the_selected_satellite_is_and_its_doppler_shift() {
    let screen = text(&screen(&app(), &footer(t(13, 15, 0))));

    assert!(screen.contains("299.5"), "az missing:\n{screen}");
    assert!(screen.contains("16.3"), "el missing:\n{screen}");
    assert!(screen.contains("1183"), "range missing:\n{screen}");
    assert!(
        screen.contains("145,828,190"),
        "doppler-corrected downlink missing:\n{screen}"
    );
}

#[test]
fn the_picker_lists_transmitters_and_marks_receive_only_ones() {
    let mut app = app();
    press(&mut app, KeyCode::Down); // RS-44's pass

    let screen = text(&screen(&app, &footer(t(19, 44, 0))));

    assert!(screen.contains("Mode V/U - Transponder"), "{screen}");
    assert!(screen.contains("Mode U - Beacon"), "{screen}");
    assert!(screen.contains("RX only"), "{screen}");
}

#[test]
fn the_footer_shows_the_clock_data_ages_and_rig_state() {
    let screen = text(&screen(&app(), &footer(t(13, 15, 0))));

    assert!(screen.contains("13:15:00Z"), "{screen}");
    assert!(screen.contains("TLEs 3h"), "{screen}");
    assert!(screen.contains("SatNOGS 5h"), "{screen}");
    assert!(screen.contains("rig: not connected"), "{screen}");
}

#[test]
fn old_tles_are_flagged_and_missing_data_says_so() {
    let mut stale = footer(t(13, 15, 0));
    stale.tles_stale = true;
    stale.tle_age = Some(Duration::days(8));
    stale.transmitter_age = None;

    let screen = text(&screen(&app(), &stale));

    assert!(screen.contains("TLEs 8d STALE"), "{screen}");
    assert!(screen.contains("SatNOGS never"), "{screen}");
}

#[test]
fn tracking_is_announced_with_the_chosen_transmitter_and_nudge() {
    let mut app = app();
    press(&mut app, KeyCode::Char('t'));

    let before = text(&screen(&app, &footer(t(13, 15, 0))));
    assert!(before.contains("TRACKING"), "{before}");
    assert!(before.contains("Mode V APRS"), "{before}");

    let mut linear = self::app();
    press(&mut linear, KeyCode::Down);
    press(&mut linear, KeyCode::Tab);
    press(&mut linear, KeyCode::Down);
    press(&mut linear, KeyCode::Char('t'));
    press(&mut linear, KeyCode::Char(']'));
    let nudged = text(&screen(&linear, &footer(t(19, 44, 0))));
    assert!(nudged.contains("+1000 Hz"), "{nudged}");
}

#[test]
fn a_status_message_is_shown() {
    let mut app = app();
    app.set_status(Some("rig: connection refused".to_string()));

    let screen = text(&screen(&app, &footer(t(13, 15, 0))));

    assert!(screen.contains("rig: connection refused"), "{screen}");
}
