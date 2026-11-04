//! rig-tui: a terminal app for any rig behind a `rigctld`.
//!
//! `App` is pure (key and rig events in, [`Command`]s out); [`Rig`] does the I/O; `main` is a
//! thin loop between them.

mod activity;
mod app;
mod config;
mod config_dialog;
mod cw;
mod general;
pub mod geocode;
pub mod keys;
pub mod layout;
mod logging;
mod panel;
pub mod refdata;
pub mod reference;
mod rig;
pub mod softkeys;
mod style;
mod ui;

pub use activity::{Activity, ActivityCtx, ActivityOutput};
pub use app::{
    App, Focus, Overlay, OverlayAction, RECONNECT_EVERY, SMETER_POLL_EVERY, STALE_AFTER,
    STATE_POLL_EVERY,
};
pub use config::{Config, config_path, default_log_path};
pub use cw::{Line, Source, TbError, TbReply, parse_tb};
pub use general::General;
pub use panel::{LogDefaults, LogMode, Role};
pub use rig::{Command, Feature, Rig, RigEvent, RigMode};
pub use ui::render;
