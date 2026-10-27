//! Downloading TLEs from CelesTrak and transmitter data from SatNOGS.
//!
//! Blocking, so callers run it on a background thread. Every download is checked to really be
//! the expected kind of data before it is returned, so a maintenance page or an empty answer can
//! never replace a good cache.

use std::fmt;
use std::time::Duration;

use crate::satnogs::parse_transmitters;

const CELESTRAK: &str = "https://celestrak.org";
const SATNOGS: &str = "https://db.satnogs.org";
const USER_AGENT: &str = concat!("hamtools-sat-tui/", env!("CARGO_PKG_VERSION"));
/// The full SatNOGS transmitter list is several megabytes; refuse anything absurd.
const MAX_BODY_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug)]
pub struct FetchError(String);

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for FetchError {}

pub struct Fetcher {
    celestrak: String,
    satnogs: String,
    agent: ureq::Agent,
}

impl Default for Fetcher {
    fn default() -> Self {
        Self::with_base_urls(CELESTRAK, SATNOGS)
    }
}

impl Fetcher {
    pub fn new() -> Self {
        Self::default()
    }

    /// Point at other servers (for tests).
    pub fn with_base_urls(celestrak: impl Into<String>, satnogs: impl Into<String>) -> Self {
        let agent = ureq::Agent::config_builder()
            .user_agent(USER_AGENT)
            .timeout_connect(Some(Duration::from_secs(10)))
            .timeout_global(Some(Duration::from_secs(60)))
            .build()
            .into();
        Self {
            celestrak: celestrak.into(),
            satnogs: satnogs.into(),
            agent,
        }
    }

    fn get(&self, who: &str, url: &str) -> Result<String, FetchError> {
        let mut response = self
            .agent
            .get(url)
            .call()
            .map_err(|e| FetchError(format!("{who}: {e}")))?;
        response
            .body_mut()
            .with_config()
            .limit(MAX_BODY_BYTES)
            .read_to_string()
            .map_err(|e| FetchError(format!("{who}: {e}")))
    }

    /// CelesTrak's amateur-radio group as 3-line TLEs.
    pub fn tles(&self) -> Result<String, FetchError> {
        let url = format!(
            "{}/NORAD/elements/gp.php?GROUP=amateur&FORMAT=tle",
            self.celestrak
        );
        let body = self.get("CelesTrak", &url)?;
        match orbit::parse_tles(&body) {
            Ok(sats) if !sats.is_empty() => Ok(body),
            Ok(_) => Err(FetchError("CelesTrak: no TLEs in the answer".into())),
            Err(e) => Err(FetchError(format!("CelesTrak: {e}"))),
        }
    }

    /// Every SatNOGS DB transmitter record, as JSON.
    pub fn transmitters(&self) -> Result<String, FetchError> {
        let url = format!("{}/api/transmitters/?format=json", self.satnogs);
        let body = self.get("SatNOGS", &url)?;
        match parse_transmitters(&body) {
            Ok(list) if !list.is_empty() => Ok(body),
            Ok(_) => Err(FetchError("SatNOGS: no transmitters in the answer".into())),
            Err(e) => Err(FetchError(format!("SatNOGS: {e}"))),
        }
    }
}
