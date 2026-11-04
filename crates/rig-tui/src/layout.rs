//! The one breakpoint function: which panes a terminal of a given size has room for.
//!
//! Screens grow along two independent axes. Width adds columns (a second from 128, a third from
//! 240); height adds panes (the band plan strip from 30 rows, the key hints block and a stacked
//! list of today's Contacts from 40). Panes appear in a fixed order and drop in reverse, never
//! wrapping. Below 80x24 there is no screen, only a notice.

/// The smallest window every Activity works in.
pub const MIN_WIDTH: u16 = 80;
pub const MIN_HEIGHT: u16 = 24;
/// Width at which a second column appears.
pub const SECOND_COLUMN_AT: u16 = 128;
/// Width at which a third column appears.
pub const THIRD_COLUMN_AT: u16 = 240;
/// Height at which the band plan strip appears.
pub const BANDPLAN_STRIP_AT: u16 = 30;
/// Height at which the key hints block and the stacked today's-contacts list appear.
pub const KEY_HINTS_AT: u16 = 40;

/// What a window of a given size has room for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    /// Below the floor: show only the notice.
    TooSmall {
        width: u16,
        height: u16,
    },
    Fits(Panes),
}

/// The panes that fit, along both axes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Panes {
    /// 1, 2 or 3.
    pub columns: u8,
    pub bandplan_strip: bool,
    pub key_hints: bool,
    pub stacked_contacts: bool,
}

impl Panes {
    /// The smallest screen: one column, nothing optional.
    pub const MINIMAL: Panes = Panes {
        columns: 1,
        bandplan_strip: false,
        key_hints: false,
        stacked_contacts: false,
    };

    /// Whether General shows its today's-contacts pane: from 128 columns or 40 rows.
    pub fn todays_contacts(&self) -> bool {
        self.columns >= 2 || self.stacked_contacts
    }
}

impl Default for Panes {
    fn default() -> Self {
        Self::MINIMAL
    }
}

pub fn fit(width: u16, height: u16) -> Fit {
    if width < MIN_WIDTH || height < MIN_HEIGHT {
        return Fit::TooSmall { width, height };
    }
    Fit::Fits(Panes {
        columns: 1 + u8::from(width >= SECOND_COLUMN_AT) + u8::from(width >= THIRD_COLUMN_AT),
        bandplan_strip: height >= BANDPLAN_STRIP_AT,
        key_hints: height >= KEY_HINTS_AT,
        stacked_contacts: height >= KEY_HINTS_AT,
    })
}

/// The notice shown below the floor.
pub fn too_small_notice(width: u16, height: u16) -> String {
    format!("Window too small: needs {MIN_WIDTH}x{MIN_HEIGHT}, have {width}x{height}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panes(width: u16, height: u16) -> Panes {
        match fit(width, height) {
            Fit::Fits(p) => p,
            other => panic!("{width}x{height} should fit, got {other:?}"),
        }
    }

    #[test]
    fn below_the_floor_is_too_small() {
        for (w, h) in [(79, 24), (80, 23), (79, 23), (0, 0), (200, 10), (40, 60)] {
            assert_eq!(
                fit(w, h),
                Fit::TooSmall {
                    width: w,
                    height: h
                }
            );
        }
    }

    #[test]
    fn the_floor_is_one_bare_column() {
        assert_eq!(panes(80, 24), Panes::MINIMAL);
        assert_eq!(panes(127, 29), Panes::MINIMAL);
    }

    #[test]
    fn width_adds_columns_at_128_and_240() {
        assert_eq!(panes(127, 24).columns, 1);
        assert_eq!(panes(128, 24).columns, 2);
        assert_eq!(panes(239, 24).columns, 2);
        assert_eq!(panes(240, 24).columns, 3);
    }

    #[test]
    fn height_adds_the_strip_at_30_and_hints_and_contacts_at_40() {
        let p = panes(80, 29);
        assert!(!p.bandplan_strip && !p.key_hints && !p.stacked_contacts);
        let p = panes(80, 30);
        assert!(p.bandplan_strip && !p.key_hints && !p.stacked_contacts);
        let p = panes(80, 39);
        assert!(p.bandplan_strip && !p.key_hints && !p.stacked_contacts);
        let p = panes(80, 40);
        assert!(p.bandplan_strip && p.key_hints && p.stacked_contacts);
    }

    #[test]
    fn the_axes_are_independent() {
        let p = panes(128, 24);
        assert_eq!(
            (p.columns, p.bandplan_strip, p.key_hints),
            (2, false, false)
        );
        let p = panes(80, 48);
        assert_eq!((p.columns, p.bandplan_strip, p.key_hints), (1, true, true));
    }

    #[test]
    fn the_operators_measured_sizes() {
        let p = panes(83, 47);
        assert_eq!(p.columns, 1);
        assert!(p.bandplan_strip && p.key_hints && p.stacked_contacts);
        let p = panes(172, 48);
        assert_eq!(p.columns, 2);
        assert!(p.bandplan_strip && p.key_hints && p.stacked_contacts);
    }

    #[test]
    fn todays_contacts_from_128_columns_or_40_rows() {
        assert!(!panes(127, 39).todays_contacts());
        assert!(panes(128, 24).todays_contacts());
        assert!(panes(80, 40).todays_contacts());
    }

    #[test]
    fn the_notice_names_the_needed_and_current_size() {
        assert_eq!(
            too_small_notice(79, 30),
            "Window too small: needs 80x24, have 79x30"
        );
    }
}
