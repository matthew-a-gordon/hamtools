//! Pass times are Skyfield's (see the orbit crate's tests): ISS 13:14:10, 05:57:35, 07:34:20,
//! 09:12:50, 10:50:13 and RS-44 19:40:27, 21:34:12, 05:35:30, 07:26:49, 09:22:21 between
//! 2026-10-26 12:00Z and 2026-10-27 12:00Z above 10 degrees.

use chrono::{DateTime, TimeZone, Utc};
use orbit::{Observer, parse_tles};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use sat_tui::app::{App, Command, Settings};
use sat_tui::satnogs::{Transmitter, parse_transmitters};

const ISS: &str = "ISS (ZARYA)";
const RS44: &str = "RS-44 & BREEZE-KM R/B";

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 26, 12, 0, 0).unwrap()
}

fn settings(watch: &[u32]) -> Settings {
    Settings {
        watch: watch.to_vec(),
        horizon_hours: 24,
        min_elevation_deg: 10.0,
        short_pass_secs: 180,
    }
}

fn transmitters() -> Vec<Transmitter> {
    parse_transmitters(include_str!("fixtures/satnogs_transmitters.json")).unwrap()
}

fn app_with(tx: Vec<Transmitter>, watch: &[u32]) -> App {
    let sats = parse_tles(include_str!("fixtures/sample.tle")).unwrap();
    let observer = Observer {
        lat_deg: 41.67,
        lon_deg: -85.71,
        alt_m: 250.0,
    };
    App::new(sats, tx, observer, settings(watch), now())
}

fn app() -> App {
    app_with(transmitters(), &[])
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn press(app: &mut App, code: KeyCode) -> Option<Command> {
    app.handle_key(key(code))
}

mod passes {
    use super::*;

    #[test]
    fn every_pass_in_the_window_is_listed_in_time_order() {
        let rows = app().rows();

        let names: Vec<_> = rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(
            names,
            [ISS, RS44, RS44, RS44, ISS, RS44, ISS, ISS, RS44, ISS]
        );
        let first = &rows[0];
        assert!(
            (first.pass.aos - Utc.with_ymd_and_hms(2026, 10, 26, 13, 14, 10).unwrap())
                .num_seconds()
                .abs()
                <= 2
        );
        assert!((first.pass.max_elevation_deg - 69.815).abs() < 0.1);
    }

    #[test]
    fn a_watch_list_limits_the_satellites() {
        let rows = app_with(transmitters(), &[44909]).rows();

        assert_eq!(rows.len(), 5);
        assert!(rows.iter().all(|r| r.name == RS44));
    }

    #[test]
    fn a_satellite_with_no_usable_transmitter_is_left_out() {
        let without_iss: Vec<_> = transmitters()
            .into_iter()
            .filter(|t| t.norad_id != 25544)
            .collect();

        let rows = app_with(without_iss, &[]).rows();

        assert_eq!(rows.len(), 5);
        assert!(rows.iter().all(|r| r.name == RS44));
    }
}

mod navigation {
    use super::*;

    #[test]
    fn up_and_down_move_the_selected_pass_and_stop_at_the_ends() {
        let mut app = app();
        assert_eq!(app.selected_row(), Some(0));

        press(&mut app, KeyCode::Up);
        assert_eq!(app.selected_row(), Some(0));

        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Char('j'));
        assert_eq!(app.selected_row(), Some(2));
        press(&mut app, KeyCode::Char('k'));
        assert_eq!(app.selected_row(), Some(1));

        for _ in 0..30 {
            press(&mut app, KeyCode::Down);
        }
        assert_eq!(app.selected_row(), Some(9));
    }

    #[test]
    fn the_selected_pass_offers_its_usable_supported_transmitters() {
        let app = app();

        let list = app.transmitter_choices();

        let names: Vec<_> = list.iter().map(|t| t.description.as_str()).collect();
        assert_eq!(names[0], "Mode V APRS");
        assert!(names.contains(&"Mode V/V FM (crew R2+3)"));
        // Inactive, and a digital mode the radio cannot demodulate:
        assert!(!names.contains(&"Tomsk-TPU-120"));
        assert!(!names.contains(&"ISS CCSDS Data 1"));
    }

    #[test]
    fn m_also_shows_the_ones_the_radio_cannot_demodulate() {
        let mut app = app();

        press(&mut app, KeyCode::Char('m'));

        let list = app.transmitter_choices();
        let qpsk = list
            .iter()
            .find(|t| t.description == "ISS CCSDS Data 1")
            .expect("shown");
        assert!(!qpsk.supported);
        assert!(
            list.iter()
                .find(|t| t.description == "Mode V APRS")
                .unwrap()
                .supported
        );
    }

    #[test]
    fn tab_moves_the_cursor_between_the_pass_list_and_the_transmitters() {
        let mut app = app();

        press(&mut app, KeyCode::Tab);
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Down);

        assert_eq!(app.selected_row(), Some(0));
        assert_eq!(app.selected_transmitter(), Some(2));

        press(&mut app, KeyCode::Tab);
        press(&mut app, KeyCode::Down);
        assert_eq!(app.selected_row(), Some(1));
    }

    #[test]
    fn choosing_another_pass_starts_again_at_its_first_transmitter() {
        let mut app = app();
        press(&mut app, KeyCode::Tab);
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Tab);

        press(&mut app, KeyCode::Down);

        assert_eq!(app.selected_transmitter(), Some(0));
    }

    #[test]
    fn q_and_escape_quit_and_r_asks_for_a_refresh() {
        assert_eq!(press(&mut app(), KeyCode::Char('q')), Some(Command::Quit));
        assert_eq!(press(&mut app(), KeyCode::Esc), Some(Command::Quit));
        assert_eq!(
            press(&mut app(), KeyCode::Char('r')),
            Some(Command::Refresh)
        );
    }
}

mod tracking {
    use sat_tui::satnogs::RigMode;
    use sat_tui::tracker::RigCommand;

    use super::*;

    fn at(h: u32, m: u32, s: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 26, h, m, s).unwrap()
    }

    fn rx(commands: &[RigCommand]) -> Option<u64> {
        commands.iter().find_map(|c| match c {
            RigCommand::RxFreq(hz) => Some(*hz),
            _ => None,
        })
    }

    fn tx(commands: &[RigCommand]) -> Option<u64> {
        commands.iter().find_map(|c| match c {
            RigCommand::TxFreq(hz) => Some(*hz),
            _ => None,
        })
    }

    /// RS-44's 19:40 pass (row 1) with its inverting transponder (supported choice 1) selected.
    fn app_on_the_rs44_transponder() -> App {
        let mut app = app();
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Tab);
        press(&mut app, KeyCode::Down);
        assert_eq!(
            app.transmitter_choices()[1].description,
            "Mode V/U - Transponder"
        );
        app
    }

    #[test]
    fn t_with_nothing_to_track_says_so() {
        let mut app = app_with(Vec::new(), &[]);

        press(&mut app, KeyCode::Char('t'));

        assert!(app.tracking().is_none());
        assert!(
            app.status().is_some_and(|s| s.contains("transmitter")),
            "{:?}",
            app.status()
        );
    }

    #[test]
    fn nothing_is_sent_until_tracking_starts() {
        let mut app = app();

        assert_eq!(app.tick(at(13, 15, 0)).unwrap(), []);
    }

    #[test]
    fn starting_to_track_sets_up_the_radio_on_the_next_tick_then_goes_quiet() {
        let mut app = app();
        press(&mut app, KeyCode::Char('t'));
        assert_eq!(app.tracking().unwrap().satellite, ISS);

        let first = app.tick(at(13, 15, 0)).unwrap();
        let second = app.tick(at(13, 15, 0)).unwrap();

        assert_eq!(
            first[..3],
            [
                RigCommand::SatMode(true),
                RigCommand::RxMode(RigMode::Fm),
                RigCommand::TxMode(RigMode::Fm)
            ]
        );
        // Approaching the station: heard higher, transmit lower than the 145.825 MHz nominal.
        assert!(rx(&first).unwrap() > 145_825_000);
        assert!(tx(&first).unwrap() < 145_825_000);
        assert_eq!(second, []);
    }

    #[test]
    fn nudge_keys_move_the_passband_position_while_tracking() {
        let mut app = app_on_the_rs44_transponder();
        press(&mut app, KeyCode::Char('t'));
        let setup = app.tick(at(19, 44, 0)).unwrap();
        assert!(setup.contains(&RigCommand::RxMode(RigMode::Usb)));
        assert!(setup.contains(&RigCommand::TxMode(RigMode::Lsb)));
        assert!((rx(&setup).unwrap() as f64 - 435_646_322.71).abs() <= 25.0);

        press(&mut app, KeyCode::Char(']'));
        let nudged = app.tick(at(19, 44, 0)).unwrap();

        assert_eq!(nudged.len(), 2, "{nudged:?}");
        assert!((rx(&nudged).unwrap() as f64 - 435_647_322.72).abs() <= 25.0);
        assert!((tx(&nudged).unwrap() as f64 - 145_961_881.54).abs() <= 25.0);
        assert_eq!(app.tracking().unwrap().nudge_hz, 1_000);

        press(&mut app, KeyCode::Char('{'));
        assert_eq!(app.tracking().unwrap().nudge_hz, 900);
        press(&mut app, KeyCode::Char('['));
        press(&mut app, KeyCode::Char('}'));
        assert_eq!(app.tracking().unwrap().nudge_hz, 0);
    }

    #[test]
    fn t_again_stops_tracking() {
        let mut app = app();
        press(&mut app, KeyCode::Char('t'));
        app.tick(at(13, 15, 0)).unwrap();

        press(&mut app, KeyCode::Char('t'));

        assert!(app.tracking().is_none());
        assert_eq!(app.tick(at(13, 19, 0)).unwrap(), []);
    }

    #[test]
    fn s_switches_satellite_mode_off_even_when_not_tracking() {
        let mut app = app();

        press(&mut app, KeyCode::Char('s'));

        assert_eq!(
            app.tick(at(13, 15, 0)).unwrap(),
            [RigCommand::SatMode(false)]
        );
        assert_eq!(app.tick(at(13, 15, 1)).unwrap(), []);
    }

    #[test]
    fn restarting_after_a_stop_sets_up_the_radio_again() {
        let mut app = app();
        press(&mut app, KeyCode::Char('t'));
        app.tick(at(13, 15, 0)).unwrap();
        press(&mut app, KeyCode::Char('t'));
        press(&mut app, KeyCode::Char('t'));

        let again = app.tick(at(13, 15, 0)).unwrap();

        assert!(again.contains(&RigCommand::SatMode(true)), "{again:?}");
    }
}

mod reconnecting {
    use sat_tui::tracker::RigCommand;

    use super::*;

    fn at(h: u32, m: u32, s: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 26, h, m, s).unwrap()
    }

    #[test]
    fn after_a_resync_the_next_tick_sets_the_radio_up_from_scratch() {
        let mut app = app();
        press(&mut app, KeyCode::Char('t'));
        app.tick(at(13, 15, 0)).unwrap();
        assert_eq!(app.tick(at(13, 15, 0)).unwrap(), []);

        app.resync();

        let again = app.tick(at(13, 15, 0)).unwrap();
        assert!(again.contains(&RigCommand::SatMode(true)), "{again:?}");
        assert_eq!(again.len(), 5, "{again:?}");
    }

    #[test]
    fn resync_without_tracking_changes_nothing() {
        let mut app = app();

        app.resync();

        assert_eq!(app.tick(at(13, 15, 0)).unwrap(), []);
    }
}

mod durations {
    use super::*;

    // Skyfield pass lengths (LOS - AOS at 10 degrees): ISS 13:14:10.26-13:20:55.81 = 405.6 s,
    // ISS 09:12:50.98-09:17:11.27 = 260.3 s, ISS 10:50:13.39-10:55:20.44 = 307.1 s.

    fn app_at(now: DateTime<Utc>, short_pass_secs: i64) -> App {
        let sats = parse_tles(include_str!("fixtures/sample.tle")).unwrap();
        let observer = Observer {
            lat_deg: 41.67,
            lon_deg: -85.71,
            alt_m: 250.0,
        };
        let settings = Settings {
            watch: vec![],
            horizon_hours: 24,
            min_elevation_deg: 10.0,
            short_pass_secs,
        };
        App::new(sats, transmitters(), observer, settings, now)
    }

    #[test]
    fn every_row_knows_how_long_its_pass_lasts() {
        let rows = app_at(now(), 180).rows();

        let secs = rows[0].duration().num_milliseconds() as f64 / 1000.0;
        assert!((secs - 405.6).abs() < 2.0, "{secs}");
    }

    #[test]
    fn a_pass_already_under_way_still_shows_its_full_length() {
        let during = Utc.with_ymd_and_hms(2026, 10, 26, 13, 17, 0).unwrap();

        let rows = app_at(during, 180).rows();

        let secs = rows[0].duration().num_milliseconds() as f64 / 1000.0;
        assert!((secs - 405.6).abs() < 2.0, "{secs}");
        assert!(rows[0].pass.aos < during);
    }

    #[test]
    fn passes_shorter_than_the_threshold_are_flagged() {
        let rows = app_at(now(), 300).rows();

        let short: Vec<_> = rows.iter().filter(|r| r.short).collect();
        assert_eq!(
            short.len(),
            1,
            "{:?}",
            rows.iter()
                .map(|r| (r.name.clone(), r.duration()))
                .collect::<Vec<_>>()
        );
        assert_eq!(short[0].pass.aos.format("%d %H:%M").to_string(), "27 09:12");
    }

    #[test]
    fn nothing_is_flagged_when_every_pass_is_long_enough() {
        let rows = app_at(now(), 180).rows();

        assert!(rows.iter().all(|r| !r.short));
    }
}

mod countdowns {
    use sat_tui::app::Countdown;

    use super::*;

    fn at(day: u32, h: u32, m: u32, s: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, day, h, m, s).unwrap()
    }

    fn secs(d: chrono::Duration) -> f64 {
        d.num_milliseconds() as f64 / 1000.0
    }

    // ISS pass 26th 13:14:10.26 - 13:20:55.81; its last in the window is 27th 10:50-10:55.
    // RS-44's next is 26th 19:40:27.08.

    #[test]
    fn there_is_no_countdown_when_nothing_is_tracked() {
        assert_eq!(app().tracked_countdown(at(26, 13, 0, 0)), None);
    }

    #[test]
    fn before_the_pass_both_the_start_and_the_end_are_counted_down() {
        let mut app = app();
        press(&mut app, KeyCode::Char('t'));

        let Some(Countdown::Before { aos_in, los_in }) = app.tracked_countdown(at(26, 13, 0, 0))
        else {
            panic!("expected a countdown to the start");
        };

        assert!((secs(aos_in) - 850.26).abs() < 2.0, "{aos_in}");
        assert!((secs(los_in) - 1255.81).abs() < 2.0, "{los_in}");
    }

    #[test]
    fn during_the_pass_the_time_left_is_counted_down() {
        let mut app = app();
        press(&mut app, KeyCode::Char('t'));

        let Some(Countdown::During { los_in }) = app.tracked_countdown(at(26, 13, 17, 0)) else {
            panic!("expected a countdown to the end");
        };

        assert!((secs(los_in) - 235.81).abs() < 2.0, "{los_in}");
    }

    #[test]
    fn after_the_pass_it_moves_on_to_the_satellites_next_one() {
        let mut app = app();
        press(&mut app, KeyCode::Char('t'));

        let Some(Countdown::Before { aos_in, .. }) = app.tracked_countdown(at(26, 14, 0, 0)) else {
            panic!("expected the next pass");
        };

        // Next ISS pass: 27th 05:57:35.31, 15h57m35s after 14:00.
        assert!((secs(aos_in) - 57_455.31).abs() < 2.0, "{aos_in}");
    }

    #[test]
    fn when_the_satellite_has_no_more_listed_passes_it_says_so() {
        let mut app = app();
        press(&mut app, KeyCode::Char('t'));

        assert_eq!(
            app.tracked_countdown(at(27, 11, 0, 0)),
            Some(Countdown::NoFurtherPass)
        );
    }

    #[test]
    fn it_follows_the_tracked_satellite_whatever_the_cursor_is_on() {
        let mut app = app();
        press(&mut app, KeyCode::Down); // RS-44's 19:40 pass
        press(&mut app, KeyCode::Tab);
        press(&mut app, KeyCode::Char('t')); // track RS-44
        press(&mut app, KeyCode::Tab);
        press(&mut app, KeyCode::Up); // cursor back on the ISS pass
        assert_eq!(app.selected_row(), Some(0));

        let Some(Countdown::Before { aos_in, .. }) = app.tracked_countdown(at(26, 13, 15, 0))
        else {
            panic!("expected a countdown");
        };

        // RS-44 19:40:27.08 minus 13:15:00 = 6h25m27s.
        assert!((secs(aos_in) - 23_127.08).abs() < 2.0, "{aos_in}");
    }
}
