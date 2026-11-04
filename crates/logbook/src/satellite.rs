//! Satellite helpers: the ADIF extras that make a Contact a satellite Contact.
//!
//! `FREQ`/`BAND` on the Contact are the uplink (the logging station's transmit);
//! the extras here carry the downlink as `FREQ_RX`/`BAND_RX`.

use crate::band::band_for_hz;
use crate::contact::{ContactError, text};

/// AMSAT mode letter for a band, if it has one.
fn mode_letter(band: &str) -> Option<char> {
    match band {
        "15m" => Some('H'),
        "10m" => Some('A'),
        "2m" => Some('V'),
        "70cm" => Some('U'),
        "23cm" => Some('L'),
        _ => None,
    }
}

/// AMSAT-style mode such as `V/U`: uplink band letter, then downlink.
pub fn sat_mode(uplink_hz: u64, downlink_hz: u64) -> Result<String, ContactError> {
    let letter = |hz: u64| {
        let band = band_for_hz(hz).ok_or(ContactError::FrequencyOutsideBands(hz))?;
        mode_letter(band.name).ok_or(ContactError::NoSatModeLetter { band: band.name })
    };
    Ok(format!("{}/{}", letter(uplink_hz)?, letter(downlink_hz)?))
}

/// ADIF frequency in MHz with Hz resolution.
pub(crate) fn mhz(hz: u64) -> String {
    format!("{}.{:06}", hz / 1_000_000, hz % 1_000_000)
}

/// Extras for a satellite Contact: `PROP_MODE`, `SAT_NAME`, `SAT_MODE`, `FREQ_RX`, `BAND_RX`.
pub fn extras(
    sat_name: &str,
    uplink_hz: u64,
    downlink_hz: u64,
) -> Result<Vec<(String, String)>, ContactError> {
    let name = text("SAT_NAME", sat_name)?;
    if name.is_empty() {
        return Err(ContactError::Empty { field: "SAT_NAME" });
    }
    let rx_band = band_for_hz(downlink_hz).ok_or(ContactError::FrequencyOutsideBands(downlink_hz))?;
    Ok(vec![
        ("PROP_MODE".into(), "SAT".into()),
        ("SAT_NAME".into(), name.to_string()),
        ("SAT_MODE".into(), sat_mode(uplink_hz, downlink_hz)?),
        ("FREQ_RX".into(), mhz(downlink_hz)),
        ("BAND_RX".into(), rx_band.name.into()),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sat_modes() {
        assert_eq!(sat_mode(145_900_000, 435_300_000).unwrap(), "V/U");
        assert_eq!(sat_mode(435_300_000, 145_900_000).unwrap(), "U/V");
        assert_eq!(sat_mode(435_300_000, 435_800_000).unwrap(), "U/U");
        assert_eq!(sat_mode(1_267_000_000, 435_800_000).unwrap(), "L/U");
        assert_eq!(sat_mode(21_200_000, 29_400_000).unwrap(), "H/A");
    }

    #[test]
    fn sat_mode_errors() {
        assert_eq!(
            sat_mode(14_200_000, 145_900_000),
            Err(ContactError::NoSatModeLetter { band: "20m" })
        );
        assert_eq!(
            sat_mode(145_900_000, 100),
            Err(ContactError::FrequencyOutsideBands(100))
        );
    }

    #[test]
    fn extras_fields() {
        let e = extras("AO-91", 145_900_000, 435_300_123).unwrap();
        let get = |k: &str| e.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
        assert_eq!(get("PROP_MODE"), Some("SAT"));
        assert_eq!(get("SAT_NAME"), Some("AO-91"));
        assert_eq!(get("SAT_MODE"), Some("V/U"));
        assert_eq!(get("FREQ_RX"), Some("435.300123"));
        assert_eq!(get("BAND_RX"), Some("70cm"));
    }
}
