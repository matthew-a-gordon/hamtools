//! A validated station callsign.

use std::fmt;

/// A callsign: 3 to 12 letters, digits or `/`, stored uppercased.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Callsign(String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallsignError(pub String);

impl fmt::Display for CallsignError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid callsign {:?}: expected 3 to 12 letters, digits or '/'",
            self.0
        )
    }
}

impl std::error::Error for CallsignError {}

impl Callsign {
    pub fn new(raw: &str) -> Result<Self, CallsignError> {
        let call = raw.trim().to_uppercase();
        let valid = (3..=12).contains(&call.len())
            && call
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '/');
        if valid {
            Ok(Callsign(call))
        } else {
            Err(CallsignError(raw.to_string()))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Callsign {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uppercases_and_trims() {
        assert_eq!(Callsign::new(" w1aw/p ").unwrap().as_str(), "W1AW/P");
    }

    #[test]
    fn length_bounds() {
        assert!(Callsign::new("K1").is_err());
        assert!(Callsign::new("K1A").is_ok());
        assert!(Callsign::new("ABCDEFGHIJK1").is_ok());
        assert!(Callsign::new("ABCDEFGHIJK12").is_err());
    }

    #[test]
    fn rejects_other_characters() {
        assert!(Callsign::new("W1 AW").is_err());
        assert!(Callsign::new("W1AW-1").is_err());
        assert!(Callsign::new("W1AWé").is_err());
        assert!(Callsign::new("").is_err());
    }
}
