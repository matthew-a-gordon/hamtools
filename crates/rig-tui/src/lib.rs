//! rig-tui: a terminal app for any rig behind a `rigctld`.
//!
//! `App` is pure (key and rig events in, [`Command`]s out); [`Rig`] does the I/O and [`RigPool`]
//! runs it off the screen's thread; `main` is a thin loop between them.

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
mod geo;
pub mod geocode;
pub mod gps;
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
mod rig_pool;
mod rigs;
pub mod satellite;
pub mod softkeys;
mod sota;
pub mod spots;
mod spotwatch;
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
    STATE_POLL_EVERY, STATUS_TTL,
};
pub use config::{Config, config_path, default_log_path};
pub use cw::{Line, Source, TbError, TbReply, parse_tb};
pub use general::General;
pub use panel::{LogDefaults, LogMode, Role};
pub use pota::{PotaHunter, SPOTS_KEY, SPOTS_URL, spots_feed};
pub use rig::{Command, Feature, Rig, RigEvent, RigMode};
pub use rig_pool::RigPool;
pub use rigs::{
    Capabilities, Capability, LIGHT_POLL_EVERY, Split, Support, table as capability_table,
};
pub use sota::{NOTICE as SOTA_SPOTS_NOTICE, SotaChaser};
pub use tracker::{ActivationView, PlanBanner, Program, Tracker};
pub use ui::render;
