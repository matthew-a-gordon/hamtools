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

/// Render at an explicit size (the other helpers use 110x32).
fn screen_sized(app: &App, footer: &Footer, width: u16, height: u16) -> Vec<String> {
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

/// Screen rows reach across both panes; these cut a 110-column screen at the pass list's edge
/// (30% of the width, 33 columns) so assertions say which pane they mean.
const LIST_WIDTH: usize = 33;

fn left_pane(rows: &[String]) -> Vec<String> {
    rows.iter()
        .map(|r| r.chars().take(LIST_WIDTH).collect())
        .collect()
}

fn right_pane(rows: &[String]) -> String {
    rows.iter()
        .map(|r| r.chars().skip(LIST_WIDTH).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

mod list_and_detail {
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

    // ISS pass 13:14:10.26 - 13:20:55.81 (6 min 45 s, peak 69.8 degrees at 13:17:33).

    #[test]
    fn a_row_gives_start_length_and_peak_but_neither_the_end_time_nor_the_day() {
        let list = left_pane(&screen(&app(), &footer(t(13, 15, 0))));

        let row = list.iter().find(|r| r.contains("13:14")).expect("ISS row");
        assert!(row.contains("6:45"), "{row}");
        assert!(row.contains("70°"), "{row}");
        assert!(row.contains("ISS"), "{row}");
        assert!(
            !row.contains("13:20"),
            "end time belongs in the detail panel: {row}"
        );
        assert!(
            !row.contains("26"),
            "the day belongs on a separator line: {row}"
        );
    }

    #[test]
    fn the_list_has_column_headings() {
        let list = text(&left_pane(&screen(&app(), &footer(t(13, 15, 0)))));

        assert!(list.contains("start"), "{list}");
        assert!(list.contains("len"), "{list}");
        assert!(list.contains("peak"), "{list}");
    }

    #[test]
    fn each_day_gets_one_separator_line_above_its_passes() {
        let list = left_pane(&screen(&app(), &footer(t(13, 15, 0))));

        let at = |needle: &str| {
            list.iter()
                .position(|r| r.contains(needle))
                .unwrap_or_else(|| panic!("no {needle:?}"))
        };
        assert!(at("Mon 26 Oct") < at("13:14"), "{list:#?}");
        assert!(at("13:14") < at("21:34"), "{list:#?}");
        assert!(at("21:34") < at("Tue 27 Oct"), "{list:#?}");
        assert!(at("Tue 27 Oct") < at("05:35"), "{list:#?}");
        assert_eq!(list.iter().filter(|r| r.contains("Oct")).count(), 2);
    }

    #[test]
    fn short_passes_get_a_marker_and_long_ones_do_not() {
        // The ISS pass at 27th 09:12 lasts 4:20; 300 s makes it, and only it, short.
        let list = left_pane(&screen(
            &app_with_short_threshold(300),
            &footer(t(13, 15, 0)),
        ));

        let short = list
            .iter()
            .find(|r| r.contains("09:12"))
            .expect("short row");
        assert!(short.contains("4:20!"), "{short}");
        let long = list.iter().find(|r| r.contains("13:14")).expect("long row");
        assert!(!long.contains('!'), "{long}");
    }

    #[test]
    fn names_are_short_in_the_list_and_whole_in_the_detail() {
        let mut app = app();
        press(&mut app, KeyCode::Down); // RS-44's 19:40 pass
        let rows = screen(&app, &footer(t(19, 30, 0)));

        let list = text(&left_pane(&rows));
        assert!(list.contains("RS-44"), "{list}");
        assert!(!list.contains("BREEZE"), "{list}");
        assert!(
            right_pane(&rows).contains("RS-44 & BREEZE-KM R/B"),
            "{rows:#?}"
        );
    }

    #[test]
    fn the_detail_gives_name_id_window_length_and_the_time_of_the_peak() {
        let rows = screen(&app(), &footer(t(13, 0, 0)));
        let detail = right_pane(&rows);

        assert!(detail.contains("ISS (ZARYA)"), "{detail}");
        assert!(detail.contains("NORAD 25544"), "{detail}");
        assert!(detail.contains("13:14:10-13:20:55Z"), "{detail}");
        assert!(detail.contains("6:45"), "{detail}");
        assert!(detail.contains("peak 70° at 13:17:33Z"), "{detail}");
        // Range stays in the live line.
        assert!(detail.contains("Range"), "{detail}");
    }

    #[test]
    fn the_selected_row_scrolls_into_view_in_a_short_terminal() {
        let mut app = app();
        for _ in 0..9 {
            press(&mut app, KeyCode::Down);
        }

        let rows = screen_sized(&app, &footer(t(13, 15, 0)), 110, 14);

        let list = left_pane(&rows);
        let selected = list.iter().find(|r| r.contains("> ")).expect("cursor row");
        assert!(selected.contains("10:50"), "{list:#?}");
    }
}

mod tracking_strip {
    use super::*;

    #[test]
    fn tracking_has_its_own_strip_apart_from_the_live_panel() {
        let mut app = app();
        press(&mut app, KeyCode::Char('t'));

        let detail = right_pane(&screen(&app, &footer(t(13, 0, 0))));

        assert!(detail.contains("Tracking"), "{detail}");
        assert!(detail.contains("TRACKING Mode V APRS"), "{detail}");
        assert!(
            detail.contains("pass: AOS in 14:10  LOS in 20:55"),
            "{detail}"
        );
    }

    #[test]
    fn without_tracking_there_is_no_strip() {
        let detail = right_pane(&screen(&app(), &footer(t(13, 0, 0))));

        assert!(!detail.contains("Tracking"), "{detail}");
        assert!(!detail.contains("TRACKING"), "{detail}");
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

#[test]
fn nothing_in_the_live_panel_is_cut_off_on_an_80_column_terminal() {
    let rows = screen_sized(&app(), &footer(t(13, 15, 0)), 80, 24);
    let detail = rows
        .iter()
        .map(|r| r.chars().skip(26).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(detail.contains("Rate -6.56 km/s"), "{detail}");
    assert!(detail.contains("peak 70° at 13:17:33Z"), "{detail}");
    assert!(detail.contains("RX     145,828,190 Hz  FM"), "{detail}");
}
