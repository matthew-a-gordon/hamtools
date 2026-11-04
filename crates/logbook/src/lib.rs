//! Append-only ADIF Log of Contacts, shared by the hamtools programs.

mod adif;
pub mod band;
pub mod callsign;
pub mod contact;
pub mod locator;
pub mod log;
pub mod satellite;

pub use band::{BANDS, Band, band_for_hz};
pub use callsign::{Callsign, CallsignError};
pub use contact::{Contact, ContactError, ContactInput, Mode, Sideband};
pub use locator::{LocatorError, locator, validate_grid};
pub use log::{AppendError, Log};
