//! Render tests for the Contact form, the session overlay and the count pill (TestBackend).

use chrono::{DateTime, Duration, TimeZone, Utc};
use logbook::{Callsign, Log};
use orbit::{Observer, parse_tles};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use sat_tui::app::{App, Logging, Settings};
use sat_tui::satnogs::parse_transmitters;
use sat_tui::tracker::RigCommand;
use sat_tui::ui::{Footer, render};

fn t(h: u32, m: u32, s: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 26, h, m, s).unwrap()
}

fn footer(now: DateTime<Utc>) -> Footer {
    Footer {
        now,
        tle_age: Some(Duration::hours(3)),
        transmitter_age: Some(Duration::hours(5)),
        tles_stale: false,
        rig: "connected".to_string(),
    }
}

fn screen(app: &App, now: DateTime<Utc>) -> String {
    let (width, height) = (110u16, 32u16);
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| render(app, &footer(now), frame))
        .unwrap();
    terminal
        .backend()
        .buffer()
        .content()
        .chunks(width as usize)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

fn press(app: &mut App, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, KeyCode::Char(c));
    }
}

fn tracking_app(name: &str) -> (App, Vec<RigCommand>) {
    let dir = std::env::temp_dir().join(format!("sat-tui-render-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut app = App::new(
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
    );
    app.set_logging(Some(Logging::new(Log::new(
        dir.join("log.adi"),
        Callsign::new("W1AW").unwrap(),
    ))));
    press(&mut app, KeyCode::Char('t'));
    app.set_rig_live(true);
    let sent = app.tick(t(13, 15, 0)).unwrap();
    (app, sent)
}

fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[test]
fn the_form_shows_the_snapshot_and_the_radio_now_and_the_own_grid_in_the_title() {
    let (mut app, sent) = tracking_app("form");
    let rx = sent
        .iter()
        .find_map(|c| {
            if let RigCommand::RxFreq(h) = c {
                Some(*h)
            } else {
                None
            }
        })
        .unwrap();
    let tx = sent
        .iter()
        .find_map(|c| {
            if let RigCommand::TxFreq(h) = c {
                Some(*h)
            } else {
                None
            }
        })
        .unwrap();
    press(&mut app, KeyCode::Char('l'));
    type_text(&mut app, "k9xyz");
    // Tracking carries on: the radio moves, the stamped values do not.
    let later = app.tick(t(13, 18, 0)).unwrap();
    let rx_later = later
        .iter()
        .find_map(|c| {
            if let RigCommand::RxFreq(h) = c {
                Some(*h)
            } else {
                None
            }
        })
        .unwrap();

    let screen = screen(&app, t(13, 15, 45));

    assert!(screen.contains("ISS (ZARYA)"), "{screen}");
    assert!(screen.contains("EN71DQ"), "own grid in title:\n{screen}");
    assert!(screen.contains("13:15:00Z"), "ON time:\n{screen}");
    assert!(screen.contains("0:45"), "seconds open:\n{screen}");
    assert!(
        screen.contains(&format!("RX {} Hz", grouped(rx))),
        "{screen}"
    );
    assert!(
        screen.contains(&format!("TX {} Hz", grouped(tx))),
        "{screen}"
    );
    assert!(
        screen.contains(&format!("radio now RX {} Hz", grouped(rx_later))),
        "{screen}"
    );
    assert!(screen.contains("K9XYZ"), "typed call:\n{screen}");
    assert!(screen.contains("FM"), "mode:\n{screen}");
}

#[test]
fn the_form_shows_its_error() {
    let (mut app, _) = tracking_app("form-error");
    press(&mut app, KeyCode::Char('l'));
    press(&mut app, KeyCode::Enter);

    let screen = screen(&app, t(13, 15, 5));

    assert!(screen.contains("Call is required"), "{screen}");
}

#[test]
fn the_count_pill_shows_while_tracking_and_counts_this_sessions_contacts() {
    let (mut app, _) = tracking_app("pill");

    assert!(screen(&app, t(13, 15, 5)).contains("l log   L list (0)"));

    for call in ["AA1AA", "BB2BB"] {
        press(&mut app, KeyCode::Char('l'));
        type_text(&mut app, call);
        press(&mut app, KeyCode::Enter);
    }

    assert!(screen(&app, t(13, 16, 5)).contains("l log   L list (2)"));
}

#[test]
fn there_is_no_pill_when_not_tracking_or_while_the_form_is_open() {
    let (mut app, _) = tracking_app("no-pill");
    press(&mut app, KeyCode::Char('l'));
    assert!(!screen(&app, t(13, 15, 5)).contains("L list"));
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Char('t'));

    assert!(!screen(&app, t(13, 15, 5)).contains("L list"));
}

#[test]
fn the_overlay_lists_contacts_newest_last_and_scrolls() {
    let (mut app, _) = tracking_app("overlay");
    for i in 0..40 {
        press(&mut app, KeyCode::Char('l'));
        type_text(&mut app, &format!("W{}ABC", i % 10));
        press(&mut app, KeyCode::Tab);
        type_text(&mut app, "EN52");
        press(&mut app, KeyCode::Enter);
    }
    press(&mut app, KeyCode::Char('l'));
    type_text(&mut app, "ZZ9LAST");
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Char('L'));

    let newest = screen(&app, t(13, 16, 0));
    assert!(newest.contains("Session"), "{newest}");
    assert!(newest.contains("ZZ9LAST"), "{newest}");
    assert!(newest.contains("13:15"), "time:\n{newest}");
    assert!(newest.contains("EN52"), "grid:\n{newest}");
    assert!(newest.contains("59"), "rst:\n{newest}");
    assert!(newest.contains("ISS"), "sat:\n{newest}");

    for _ in 0..10 {
        press(&mut app, KeyCode::Up);
    }
    let older = screen(&app, t(13, 16, 0));
    assert!(!older.contains("Z ZZ9LAST"), "scrolled away:\n{older}");

    let last = newest
        .lines()
        .position(|l| l.contains("Z ZZ9LAST"))
        .unwrap();
    let above = newest.lines().position(|l| l.contains("W9ABC")).unwrap();
    assert!(above < last, "newest last:\n{newest}");
}
