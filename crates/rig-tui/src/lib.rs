//! rig-tui: a terminal app for any rig behind a `rigctld`.
//!
//! `App` is pure (key and rig events in, [`Command`]s out); [`Rig`] does the I/O; `main` is a
//! thin loop between them.

mod app;
mod config;
mod config_dialog;
mod cw;
pub mod geocode;
mod panel;
mod rig;
mod ui;

pub use app::{App, Focus, Overlay, OverlayAction, RECONNECT_EVERY, STATE_POLL_EVERY};
pub use config::{Config, config_path};
pub use cw::{Line, Source, TbError, TbReply, parse_tb};
pub use panel::{LogDefaults, LogMode};
pub use rig::{Command, Rig, RigEvent, RigMode};
pub use ui::render;
