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
            short_pass_secs: 180,
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

mod pass_list_columns {
    use super::*;

    fn app_with_short_threshold(secs: i64) -> App {
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
                short_pass_secs: secs,
            },
            t(12, 0, 0),
        )
    }

    // ISS pass 13:14:10.26 - 13:20:55.81 (6 min 45 s, durations are shown rounded down).

    #[test]
    fn each_row_shows_start_end_and_how_long_the_pass_lasts() {
        let rows = screen(&app(), &footer(t(13, 15, 0)));

        let row = rows
            .iter()
            .find(|r| r.contains("ISS (ZARYA)"))
            .expect("ISS row");
        assert!(row.contains("13:14"), "{row}");
        assert!(row.contains("13:20"), "{row}");
        assert!(row.contains("6:45"), "{row}");
        assert!(row.contains("70°"), "{row}");
    }

    #[test]
    fn short_passes_get_a_marker_and_long_ones_do_not() {
        // The ISS pass at 27th 09:12 lasts 4:20; 300 s makes it, and only it, short.
        let rows = screen(&app_with_short_threshold(300), &footer(t(13, 15, 0)));

        let short = rows
            .iter()
            .find(|r| r.contains("27 09:12"))
            .expect("short row");
        assert!(short.contains("4:20!"), "{short}");
        let long = rows
            .iter()
            .find(|r| r.contains("26 13:14"))
            .expect("long row");
        assert!(!long.contains('!'), "{long}");
    }

    #[test]
    fn long_satellite_names_are_cut_to_fit_without_breaking_the_columns() {
        let rows = screen(&app(), &footer(t(13, 15, 0)));

        let row = rows
            .iter()
            .find(|r| r.contains("19:40"))
            .expect("RS-44 row");
        // Still shows the times and a recognisable name, inside the pane border.
        assert!(row.contains("RS-44"), "{row}");
        assert!(row.contains("16:33"), "{row}");
        assert!(row.matches('│').count() >= 2, "{row}");
    }
}

mod tracked_countdown {
    use super::*;

    fn tracking_iss() -> App {
        let mut app = app();
        press(&mut app, KeyCode::Char('t'));
        app
    }

    // ISS pass 26th 13:14:10.26 - 13:20:55.81; next 27th 05:57:35.31.

    #[test]
    fn before_the_pass_shows_time_to_start_and_to_end() {
        let screen = text(&screen(&tracking_iss(), &footer(t(13, 0, 0))));

        assert!(
            screen.contains("pass: AOS in 14:10  LOS in 20:55"),
            "{screen}"
        );
    }

    #[test]
    fn during_the_pass_shows_time_left() {
        let screen = text(&screen(&tracking_iss(), &footer(t(13, 17, 0))));

        assert!(screen.contains("pass: LOS in 3:55"), "{screen}");
        assert!(!screen.contains("pass: AOS in"), "{screen}");
    }

    #[test]
    fn long_waits_are_shown_in_hours() {
        let screen = text(&screen(&tracking_iss(), &footer(t(14, 0, 0))));

        assert!(screen.contains("pass: AOS in 15:57:35"), "{screen}");
    }

    #[test]
    fn it_stays_on_the_tracked_satellite_when_the_cursor_moves_away() {
        let mut app = tracking_iss();
        press(&mut app, KeyCode::Tab);
        press(&mut app, KeyCode::Tab);
        press(&mut app, KeyCode::Down); // cursor on RS-44's pass

        let screen = text(&screen(&app, &footer(t(13, 0, 0))));

        assert!(screen.contains("pass: AOS in 14:10"), "{screen}");
    }

    #[test]
    fn nothing_is_shown_when_not_tracking() {
        let screen = text(&screen(&app(), &footer(t(13, 0, 0))));

        assert!(!screen.contains("pass: "), "{screen}");
    }

    #[test]
    fn the_selected_passs_own_countdown_uses_the_same_format() {
        let screen = text(&screen(&app(), &footer(t(13, 0, 0))));

        assert!(screen.contains("AOS in 14:10"), "{screen}");
    }
}
