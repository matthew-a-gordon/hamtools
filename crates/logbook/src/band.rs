//! The ADIF amateur band table, 160 m through 23 cm.

/// One amateur band: ADIF name and inclusive edges in Hz.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Band {
    pub name: &'static str,
    pub lower_hz: u64,
    pub upper_hz: u64,
}

const fn band(name: &'static str, lower_khz: u64, upper_khz: u64) -> Band {
    Band {
        name,
        lower_hz: lower_khz * 1000,
        upper_hz: upper_khz * 1000,
    }
}

pub const BANDS: [Band; 17] = [
    band("160m", 1_800, 2_000),
    band("80m", 3_500, 4_000),
    band("60m", 5_060, 5_450),
    band("40m", 7_000, 7_300),
    band("30m", 10_100, 10_150),
    band("20m", 14_000, 14_350),
    band("17m", 18_068, 18_168),
    band("15m", 21_000, 21_450),
    band("12m", 24_890, 24_990),
    band("10m", 28_000, 29_700),
    band("6m", 50_000, 54_000),
    band("4m", 70_000, 71_000),
    band("2m", 144_000, 148_000),
    band("1.25m", 222_000, 225_000),
    band("70cm", 420_000, 450_000),
    band("33cm", 902_000, 928_000),
    band("23cm", 1_240_000, 1_300_000),
];

/// The band containing `hz` (edges inclusive), or `None` in a gap or outside the table.
pub fn band_for_hz(hz: u64) -> Option<&'static Band> {
    BANDS
        .iter()
        .find(|b| (b.lower_hz..=b.upper_hz).contains(&hz))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(hz: u64) -> Option<&'static str> {
        band_for_hz(hz).map(|b| b.name)
    }

    #[test]
    fn every_band_includes_both_edges_and_excludes_just_outside() {
        for b in BANDS {
            assert_eq!(name(b.lower_hz), Some(b.name));
            assert_eq!(name(b.upper_hz), Some(b.name));
            assert_ne!(name(b.lower_hz - 1), Some(b.name));
            assert_ne!(name(b.upper_hz + 1), Some(b.name));
        }
    }

    #[test]
    fn known_frequencies() {
        assert_eq!(name(145_800_000), Some("2m"));
        assert_eq!(name(435_300_000), Some("70cm"));
        assert_eq!(name(14_060_000), Some("20m"));
        assert_eq!(name(1_296_000_000), Some("23cm"));
    }

    #[test]
    fn gaps_and_out_of_range() {
        assert_eq!(name(100_000_000), None); // FM broadcast, between 4 m and 2 m
        assert_eq!(name(9_000_000), None);
        assert_eq!(name(100), None);
        assert_eq!(name(2_400_000_000), None);
    }
}
