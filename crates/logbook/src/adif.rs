//! Hand-written ADI writer. Everything is ASCII, so byte and character counts coincide.

use chrono::{DateTime, Datelike, Timelike, Utc};

use crate::callsign::Callsign;
use crate::contact::Contact;
use crate::satellite::mhz;

pub(crate) const EOR: &str = "<EOR>";
pub(crate) const EOH: &str = "<EOH>";

fn field(out: &mut String, name: &str, value: &str) {
    out.push_str(&format!("<{name}:{}>{value}", value.len()));
}

fn date(t: DateTime<Utc>) -> String {
    format!("{:04}{:02}{:02}", t.year(), t.month(), t.day())
}

fn time(t: DateTime<Utc>) -> String {
    format!("{:02}{:02}{:02}", t.hour(), t.minute(), t.second())
}

/// The header written once, to a new or empty file. It opens with free text because a
/// file whose first character is `<` has no header in ADIF.
pub(crate) fn header() -> String {
    let mut out = String::from("ADIF export from hamtools\n");
    field(&mut out, "ADIF_VER", "3.1.4");
    out.push('\n');
    field(&mut out, "PROGRAMID", "hamtools");
    out.push('\n');
    field(&mut out, "PROGRAMVERSION", env!("CARGO_PKG_VERSION"));
    out.push('\n');
    out.push_str(EOH);
    out.push('\n');
    out
}

/// One record on one line, ending in `<EOR>` and a newline.
pub(crate) fn record(c: &Contact, operator: &Callsign) -> String {
    let mut out = String::new();
    let station = c.station_callsign.as_ref().unwrap_or(operator);
    field(&mut out, "STATION_CALLSIGN", station.as_str());
    field(&mut out, "OPERATOR", operator.as_str());
    field(&mut out, "CALL", &c.call);
    if let Some(g) = &c.grid {
        field(&mut out, "GRIDSQUARE", g);
    }
    if let Some(g) = &c.own_grid {
        field(&mut out, "MY_GRIDSQUARE", g.as_str());
    }
    field(&mut out, "QSO_DATE", &date(c.start));
    field(&mut out, "TIME_ON", &time(c.start));
    if date(c.end) != date(c.start) {
        field(&mut out, "QSO_DATE_OFF", &date(c.end));
    }
    field(&mut out, "TIME_OFF", &time(c.end));
    field(&mut out, "BAND", c.band);
    field(&mut out, "FREQ", &mhz(c.freq_hz));
    let (mode, submode) = c.mode.adif();
    field(&mut out, "MODE", mode);
    if let Some(s) = submode {
        field(&mut out, "SUBMODE", s);
    }
    for (name, value) in [
        ("RST_SENT", &c.rst_sent),
        ("RST_RCVD", &c.rst_rcvd),
        ("NAME", &c.name),
        ("COMMENT", &c.comment),
    ] {
        if let Some(v) = value {
            field(&mut out, name, v);
        }
    }
    for (name, value) in &c.extras {
        field(&mut out, name, value);
    }
    out.push_str(EOR);
    out.push('\n');
    out
}
