//! `m` in the band plan modal finds a usual mode on HF.

mod common;

use common::*;
use hamtools_config::LicenseClass;
use ratatui::crossterm::event::KeyCode;
use rig_tui::{App, Command, RigEvent, RigMode};

fn modal_at(hz: u64, rig_mode: &str) -> App {
    let mut app = connected_app_in(
        App::new().with_license_class(Some(LicenseClass::General)),
        rig_mode,
    );
    app.on_rig(RigEvent::Freq(hz));
    app.on_key(ctrl('b'));
    app
}

#[test]
fn m_on_hf_phone_picks_lsb_below_10_mhz_and_usb_above() {
    for (hz, mode) in [(7_200_000, RigMode::Lsb), (14_250_000, RigMode::Usb)] {
        let mut app = modal_at(hz, "CW");
        assert_eq!(
            app.on_key(key(KeyCode::Char('m'))),
            vec![Command::SetMode { mode, passband: 0 }],
            "{hz}"
        );
    }
}

#[test]
fn m_on_an_hf_cw_segment_picks_cw() {
    let mut app = modal_at(7_020_000, "USB");
    assert_eq!(
        app.on_key(key(KeyCode::Char('m'))),
        vec![Command::SetMode {
            mode: RigMode::Cw,
            passband: 0
        }]
    );
}
