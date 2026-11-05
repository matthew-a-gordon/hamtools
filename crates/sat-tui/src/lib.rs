//! Satellite data and tracking, free of any terminal UI: SatNOGS/CelesTrak downloads, the cache,
//! the observer position and the Doppler tracker. The Satellite Activity of `rig-tui` builds the
//! screens and performs the rig commands.

pub mod cache;
pub mod fetch;
pub mod observer;
pub mod satnogs;
pub mod tracker;
