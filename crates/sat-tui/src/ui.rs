//! Drawing the app: pass list, live panel, transmitter picker and footer.

use chrono::{DateTime, Duration, Utc};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{App, Live};
use crate::cache::format_age;

/// Everything shown that the [`App`] does not own: the clock, data ages and rig connection.
#[derive(Debug, Clone)]
pub struct Footer {
    pub now: DateTime<Utc>,
    pub tle_age: Option<Duration>,
    pub transmitter_age: Option<Duration>,
    pub tles_stale: bool,
    pub rig: String,
}

/// `145828190` as `145,828,190`.
fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn countdown(secs: i64) -> String {
    let secs = secs.max(0);
    format!("{}m{:02}s", secs / 60, secs % 60)
}

pub fn render(app: &App, footer: &Footer, frame: &mut Frame) {
    let [main, foot] =
        Layout::vertical([Constraint::Min(8), Constraint::Length(2)]).areas(frame.area());
    let [left, right] =
        Layout::horizontal([Constraint::Length(40), Constraint::Min(30)]).areas(main);
    let [live_area, picker_area] =
        Layout::vertical([Constraint::Length(9), Constraint::Min(4)]).areas(right);

    render_passes(app, left, frame);
    render_live(app, footer.now, live_area, frame);
    render_picker(app, picker_area, frame);
    render_footer(app, footer, foot, frame);
}

fn render_passes(app: &App, area: Rect, frame: &mut Frame) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Passes (UTC)")
        .border_style(border(!app.picker_focused()));
    let inner = block.inner(area);
    let rows = app.rows();
    let selected = app.selected_row().unwrap_or(0);
    let height = inner.height as usize;
    let first = (selected + 1).saturating_sub(height.max(1));
    let lines: Vec<Line> = rows
        .iter()
        .enumerate()
        .skip(first)
        .take(height)
        .map(|(i, row)| {
            let cursor = if i == selected { ">" } else { " " };
            let text = format!(
                "{cursor} {}Z {:>3.0}° {}",
                row.pass.aos.format("%d %H:%M"),
                row.pass.max_elevation_deg,
                row.name
            );
            let style = if i == selected {
                Style::default().add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            Line::from(Span::styled(text, style))
        })
        .collect();
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn border(active: bool) -> Style {
    if active {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    }
}

fn render_live(app: &App, now: DateTime<Utc>, area: Rect, frame: &mut Frame) {
    let block = Block::default().borders(Borders::ALL).title("Live");
    let mut lines: Vec<Line> = Vec::new();
    if let Some(tracking) = app.tracking() {
        lines.push(Line::from(Span::styled(
            format!(
                "TRACKING {}  nudge {:+} Hz",
                tracking.transmitter, tracking.nudge_hz
            ),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )));
    }
    if let (Some(row), Some(live)) = (app.selected_pass(), app.live(now)) {
        lines.push(Line::from(format!(
            "{}  (NORAD {})",
            row.name, row.norad_id
        )));
        lines.push(Line::from(format!(
            "Az {:.1}°  El {:.1}°  Range {:.0} km  Rate {:+.2} km/s",
            live.azimuth_deg, live.elevation_deg, live.range_km, live.range_rate_km_s
        )));
        lines.push(Line::from(if now < row.pass.aos {
            format!("AOS in {}", countdown((row.pass.aos - now).num_seconds()))
        } else if now <= row.pass.los {
            format!("LOS in {}", countdown((row.pass.los - now).num_seconds()))
        } else {
            "pass over".to_string()
        }));
        lines.extend(frequency_lines(&live));
    } else {
        lines.push(Line::from("no passes"));
    }
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn frequency_lines(live: &Live) -> Vec<Line<'static>> {
    let Some(plan) = &live.plan else {
        return vec![Line::from("pick a transmitter (Tab)")];
    };
    let mut lines = Vec::new();
    if let Some(hz) = live.rx_hz {
        lines.push(Line::from(format!(
            "RX {:>15} Hz  {}",
            grouped(hz),
            plan.rx_mode.name()
        )));
    }
    match (live.tx_hz, plan.tx_mode) {
        (Some(hz), Some(mode)) => {
            lines.push(Line::from(format!(
                "TX {:>15} Hz  {}",
                grouped(hz),
                mode.name()
            )));
        }
        _ => lines.push(Line::from("TX   (receive only)")),
    }
    lines
}

fn render_picker(app: &App, area: Rect, frame: &mut Frame) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Transmitters (Tab, t track, m unsupported)")
        .border_style(border(app.picker_focused()));
    let inner = block.inner(area);
    let choices = app.transmitter_choices();
    let selected = app.selected_transmitter().unwrap_or(0);
    let height = inner.height as usize;
    let first = (selected + 1).saturating_sub(height.max(1));
    let tracked = app.tracking().map(|t| t.transmitter);
    let lines: Vec<Line> = choices
        .iter()
        .enumerate()
        .skip(first)
        .take(height)
        .map(|(i, choice)| {
            let cursor = if i == selected && app.picker_focused() {
                ">"
            } else {
                " "
            };
            let mark = if tracked.as_deref() == Some(choice.description.as_str()) {
                "*"
            } else {
                " "
            };
            let note = match &choice.plan {
                None => "  (radio cannot demodulate)",
                Some(plan) if plan.uplink_hz.is_none() => "  RX only",
                Some(_) => "",
            };
            let style = if choice.supported {
                Style::default()
            } else {
                Style::default().fg(Color::DarkGray)
            };
            Line::from(Span::styled(
                format!("{cursor}{mark}{}{note}", choice.description),
                style,
            ))
        })
        .collect();
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_footer(app: &App, footer: &Footer, area: Rect, frame: &mut Frame) {
    let hints = app.status().map_or_else(
        || {
            "q quit  r refresh  Tab switch  t track  [ ] { } nudge  s SAT off  m unsupported"
                .to_string()
        },
        str::to_string,
    );
    let age = |name: &str, age: Option<Duration>, stale: bool| match age {
        Some(age) => format!(
            "{name} {}{}",
            format_age(age),
            if stale { " STALE" } else { "" }
        ),
        None => format!("{name} never"),
    };
    let status = format!(
        "{}Z | {} | {} | rig: {}",
        footer.now.format("%H:%M:%S"),
        age("TLEs", footer.tle_age, footer.tles_stale),
        age("SatNOGS", footer.transmitter_age, false),
        footer.rig
    );
    frame.render_widget(
        Paragraph::new(vec![Line::from(hints), Line::from(status)]),
        area,
    );
}
