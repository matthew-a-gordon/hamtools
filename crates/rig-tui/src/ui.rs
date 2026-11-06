//! Layout and rendering: the strip on top, the body below, then the status and hint lines.

use std::time::{SystemTime, UNIX_EPOCH};

use hamtools_tui::theme::{accent, brand, ok};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, LineGauge, Paragraph, Wrap};

use crate::activity::ActivityCtx;
use crate::app::{App, Entry, Focus};
use crate::gps::Gps;
use crate::layout::{self, Fit};
use crate::rig::Feature;
use crate::rigs::Split;
use crate::style::{chip, dim, focus_border, hint_line, panel, red, yellow};

/// Group a frequency in Hz as `14.074.000`.
pub(crate) fn format_hz(hz: u64) -> String {
    format!(
        "{}.{:03}.{:03}",
        hz / 1_000_000,
        hz / 1_000 % 1_000,
        hz % 1_000
    )
}

pub fn render(app: &App, frame: &mut Frame) {
    let area = frame.area();
    let panes = match layout::fit(area.width, area.height) {
        Fit::Fits(panes) => panes,
        Fit::TooSmall { width, height } => {
            frame.render_widget(
                Paragraph::new(layout::too_small_notice(width, height))
                    .style(red())
                    .wrap(Wrap { trim: true }),
                area,
            );
            return;
        }
    };
    let [strip_area, body_area, status_area, hint_area] = Layout::vertical([
        Constraint::Length(4),
        Constraint::Min(3),
        Constraint::Length(1),
        Constraint::Length(2),
    ])
    .areas(frame.area());

    render_strip(app, frame, strip_area);
    app.render_body(frame, body_area, panes);

    if let Some(status) = app.status() {
        // A standing condition is marked and yellow (the strip already says "disconnected" in
        // red); a one-off message is red and times out.
        let (text, style) = if app.status_is_standing() {
            let elsewhere = app
                .connected_elsewhere()
                .map(|name| format!("{name} is connected (Ctrl-R to switch). "))
                .unwrap_or_default();
            (format!("\u{26a0} {elsewhere}{status}"), yellow())
        } else {
            (status.to_string(), red())
        };
        frame.render_widget(Paragraph::new(text).style(style), status_area);
    }
    frame.render_widget(Paragraph::new(hint_lines(hint(app))), hint_area);
    app.render_overlay(frame, frame.area());
}

fn format_step(hz: u64) -> String {
    if hz >= 1_000 {
        format!("{} kHz", hz / 1_000)
    } else {
        format!("{hz} Hz")
    }
}

/// The S-meter from hamlib's dB relative to S9: S0 at -54 dB, 6 dB per S-unit, then S9+dB.
fn format_strength(db: i32) -> String {
    if db > 0 {
        format!("S9+{db}")
    } else {
        format!("S{}", (9 + db / 6).max(0))
    }
}

/// `HH:MM:SSZ`, or dashes before the first clock reading.
fn format_utc(utc: Option<SystemTime>) -> String {
    let Some(secs) = utc
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
    else {
        return "--:--:--Z".to_string();
    };
    let day = secs % 86_400;
    format!("{:02}:{:02}:{:02}Z", day / 3_600, day / 60 % 60, day % 60)
}

fn row_one(app: &App) -> Line<'static> {
    let mut spans = Vec::new();
    match app.entry() {
        Some(Entry::Freq(text)) => spans.push(Span::styled(format!("freq> {text}_"), yellow())),
        Some(Entry::SplitFreq(text)) => {
            spans.push(Span::styled(format!("split tx> {text}_"), yellow()));
        }
        None => spans.push(Span::styled(
            app.frequency().map_or("-".to_string(), format_hz),
            Style::new().add_modifier(Modifier::BOLD),
        )),
    }
    spans.push(Span::raw("  "));
    let tx = app.ptt() == Some(true);
    let mode = match (app.rig_mode(), app.passband()) {
        (Some(mode), Some(passband)) => Some(format!("{mode} {passband}")),
        (Some(mode), None) => Some(mode.to_string()),
        _ => None,
    };
    spans.push(match mode {
        Some(text) => chip(
            &text,
            Color::White,
            if tx { Color::Red } else { Color::Blue },
        ),
        None => Span::styled("-", dim()),
    });
    spans.push(Span::raw("  "));
    spans.extend(kv("step", Span::raw(format_step(app.tuning_step()))));
    spans.extend(if app.connected() {
        kv("", Span::styled("connected", ok()))
    } else {
        kv("", Span::styled("disconnected", red()))
    });
    if app.doppler_stopped() {
        spans.push(Span::styled("Doppler stopped", yellow()));
        spans.push(Span::raw("  "));
    }
    spans.extend(format_location(app.locator(), app.gps()));
    Line::from(spans)
}

/// `label value │ `: the label dim, the value in the colour that says what it means.
fn kv(label: &str, value: Span<'static>) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    if !label.is_empty() {
        spans.push(Span::styled(format!("{label} "), dim()));
    }
    spans.push(value);
    spans.push(Span::styled(" \u{2502} ", dim()));
    spans
}

/// The Locator in use and, when gpsd has a fix, the coordinates (4 decimals, about 11 m). A
/// GPS Locator that differs from the one in use is shown so the operator knows to refresh it.
fn format_location(locator: Option<&str>, gps: Gps) -> Vec<Span<'static>> {
    let text = location_text(locator, gps);
    let warn = matches!((locator, gps), (Some(_), Gps::NoFix) | (None, _));
    vec![Span::styled(
        text,
        if warn { yellow() } else { Style::new() },
    )]
}

fn location_text(locator: Option<&str>, gps: Gps) -> String {
    let coords = |fix: &crate::gps::Fix| {
        let ns = if fix.lat_deg < 0.0 { 'S' } else { 'N' };
        let ew = if fix.lon_deg < 0.0 { 'W' } else { 'E' };
        format!("{:.4}{ns} {:.4}{ew}", fix.lat_deg.abs(), fix.lon_deg.abs())
    };
    match (locator, gps) {
        (Some(grid), Gps::Fix(fix)) => {
            let gps_grid = fix.locator();
            if gps_grid.as_str() == grid {
                format!("{grid}  {}", coords(&fix))
            } else {
                format!("{grid} (GPS {})  {}", gps_grid.as_str(), coords(&fix))
            }
        }
        (None, Gps::Fix(fix)) => format!("GPS {}  {}", fix.locator().as_str(), coords(&fix)),
        (Some(grid), Gps::NoFix) => format!("{grid}  GPS no fix"),
        (Some(grid), Gps::Absent) => grid.to_string(),
        (None, Gps::NoFix | Gps::Absent) => "unknown location".to_string(),
    }
}

/// Row two's state text (VFO, split, PTT), up to where the meter goes.
fn row_two(app: &App) -> Line<'static> {
    let mut spans = Vec::new();
    spans.extend(if app.is_unsupported(Feature::Vfo) {
        kv("", Span::styled("VFO not supported", dim()))
    } else {
        match app.vfo() {
            Some(v) => kv(
                "VFO",
                Span::styled(v.trim_start_matches("VFO").to_string(), accent()),
            ),
            None => kv("VFO", Span::styled("-", dim())),
        }
    });
    spans.extend(if app.is_unsupported(Feature::Split) {
        kv("", Span::styled("split not supported", dim()))
    } else {
        match app.split() {
            None => kv("split", Span::styled("-", dim())),
            Some(Split { enabled: false, .. }) => kv("split", Span::styled("off", dim())),
            Some(Split {
                enabled: true,
                tx_vfo,
            }) => {
                let tx = tx_vfo.trim_start_matches("VFO");
                let text = if app.is_unsupported(Feature::SplitFreq) {
                    format!("{tx} (transmit frequency not supported)")
                } else {
                    format!(
                        "{tx} {}",
                        app.transmit_frequency().map_or("-".to_string(), format_hz)
                    )
                };
                kv("split", Span::styled(text, yellow()))
            }
        }
    });
    spans.push(match app.ptt() {
        Some(true) => chip("TX", Color::White, Color::Red),
        Some(false) => Span::styled("RX", ok()),
        None => Span::styled("-", dim()),
    });
    spans.push(Span::raw("  "));
    Line::from(spans)
}

/// The live S-meter: green to S9, yellow to S9+20, red above; dim gray for the unfilled part.
/// The label is the S reading. While the rig is transmitting the S-meter means nothing, so it
/// shows empty.
fn smeter(app: &App) -> LineGauge<'static> {
    let (ratio, label, color) = match app.strength() {
        _ if app.is_unsupported(Feature::SMeter) => (0.0, "n/a".to_string(), Color::DarkGray),
        None => (0.0, "S-".to_string(), Color::DarkGray),
        Some(db) => (
            f64::from((db + 54).clamp(0, 114)) / 114.0,
            format_strength(db),
            match db {
                ..=0 => Color::Green,
                1..=20 => Color::Yellow,
                _ => Color::Red,
            },
        ),
    };
    LineGauge::default()
        .filled_symbol("\u{2501}")
        .unfilled_symbol("\u{258C}")
        .filled_style(Style::new().fg(color))
        .unfilled_style(dim())
        .ratio(ratio)
        .label(Span::styled(
            label,
            Style::new().fg(color).add_modifier(Modifier::BOLD),
        ))
}

/// The two-row rig-state strip: what the rig last reported, never what was requested. Open top
/// and bottom, titled with the Activity, the rig and the logged count.
fn render_strip(app: &App, frame: &mut Frame, area: Rect) {
    let title = Line::from(vec![
        Span::styled(format!(" {} ", app.activity_name()), brand()),
        Span::styled(
            format!("| {} | {} logged ", app.rig_name(), app.logged_count()),
            dim(),
        ),
    ]);
    let block = Block::new()
        .borders(Borders::TOP | Borders::BOTTOM)
        .border_style(focus_border(app.focus() == Focus::Strip))
        .title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let [row1, row2] = Layout::vertical([Constraint::Length(1); 2]).areas(inner);
    frame.render_widget(row_one(app), row1);
    let [state, gauge, clock] = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(24),
        Constraint::Length(11),
    ])
    .areas(row2);
    frame.render_widget(row_two(app), state);
    frame.render_widget(smeter(app), gauge);
    frame.render_widget(
        Line::styled(format!(" {}", format_utc(app.utc())), dim()).right_aligned(),
        clock,
    );
}

/// Shown in the body when the rig's mode has no panel.
pub(crate) fn render_no_panel(ctx: &ActivityCtx, frame: &mut Frame, area: Rect, show_recent: bool) {
    let mut text = match ctx.fresh_mode {
        Some(mode) => format!("No panel for {mode}"),
        None => "No mode reported yet".to_string(),
    };
    let recent = ctx.recent;
    if show_recent && !recent.is_empty() {
        text.push_str("\nRecent contacts:");
        for line in recent
            .iter()
            .take(usize::from(area.height.saturating_sub(4)))
        {
            text.push('\n');
            text.push_str(line);
        }
    }
    frame.render_widget(
        Paragraph::new(text).style(dim()).block(panel("", false)),
        area,
    );
}

/// The keys that apply to the current focus.
/// One or two rows (split by `\n`) so every key fits in 80 columns.
fn hint_lines(text: &str) -> Vec<Line<'static>> {
    text.split('\n').map(hint_line).collect()
}

fn hint(app: &App) -> &'static str {
    if app.quit_confirm_open() {
        return "y or Ctrl-C quit  n/Esc/Enter cancel";
    }
    if app.log_form_open() {
        return "Tab/Shift-Tab field  Enter save  Esc cancel  Ctrl-C quit";
    }
    if app.has_overlay() {
        return "Esc close  Ctrl-C quit";
    }
    if app.entry().is_some() {
        return "digits kHz, or MHz with '.'  Enter set  Esc cancel  Ctrl-C quit";
    }
    app.activity_hint()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gps::Fix;

    // Town-level Middlebury, IN only.
    const FIX: Gps = Gps::Fix(Fix {
        lat_deg: 41.67,
        lon_deg: -85.71,
        alt_m: None,
        mode: 3,
    });

    #[test]
    fn location_shows_locator_and_coordinates() {
        assert_eq!(
            location_text(Some("EN71DQ"), FIX),
            "EN71DQ  41.6700N 85.7100W"
        );
        assert_eq!(
            location_text(Some("EN71DR"), FIX),
            "EN71DR (GPS EN71DQ)  41.6700N 85.7100W"
        );
        assert_eq!(location_text(None, FIX), "GPS EN71DQ  41.6700N 85.7100W");
    }

    #[test]
    fn location_without_a_fix() {
        assert_eq!(location_text(Some("EN71DQ"), Gps::Absent), "EN71DQ");
        assert_eq!(
            location_text(Some("EN71DQ"), Gps::NoFix),
            "EN71DQ  GPS no fix"
        );
        assert_eq!(location_text(None, Gps::NoFix), "unknown location");
        assert_eq!(location_text(None, Gps::Absent), "unknown location");
    }
}
