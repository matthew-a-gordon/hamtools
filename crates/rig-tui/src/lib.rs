//! rig-tui: a terminal app for any rig behind a `rigctld`.
//!
//! `App` is pure (key and rig events in, [`Command`]s out); [`Rig`] does the I/O; `main` is a
//! thin loop between them.

mod activator;
mod activity;
mod app;
mod bandplan;
mod config;
mod config_dialog;
mod cw;
pub mod export;
mod export_modal;
mod general;
pub mod geocode;
pub mod keybindings;
pub mod keys;
pub mod layout;
mod logging;
mod palette;
mod panel;
mod plans;
mod pota;
pub mod refdata;
pub mod reference;
mod rig;
mod rigs;
pub mod satellite;
pub mod softkeys;
pub mod spots;
mod style;
mod tracker;
mod ui;

pub use activator::Activator;
pub use activity::{
    Activity, ActivityCtx, ActivityOutput, FormPane, FormRoute, HostEvent, HostRequest, LogPrefill,
    PlanChoice, TuneRequest, Worked,
};
pub use app::{
    App, Focus, Overlay, OverlayAction, RECONNECT_EVERY, SMETER_POLL_EVERY, STALE_AFTER,
    STATE_POLL_EVERY,
};
pub use config::{Config, config_path, default_log_path};
pub use cw::{Line, Source, TbError, TbReply, parse_tb};
pub use general::General;
pub use panel::{LogDefaults, LogMode, Role};
pub use pota::{PotaHunter, SPOTS_KEY, SPOTS_URL, spots_feed};
pub use rig::{Command, Feature, Rig, RigEvent, RigMode};
pub use rigs::{Capabilities, Capability, LIGHT_POLL_EVERY, Support, table as capability_table};
pub use tracker::{ActivationView, PlanBanner, Program, Tracker};
pub use ui::render;
