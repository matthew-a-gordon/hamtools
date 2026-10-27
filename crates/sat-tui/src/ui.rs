//! Drawing the app: pass list, live panel, transmitter picker and footer.

use chrono::{DateTime, Duration, Utc};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{App, Countdown, Live, Row};
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

/// A time span as `6:45`, or `15:57:35` once it passes an hour (whole seconds, rounded down).
pub fn format_countdown(d: Duration) -> String {
    let secs = d.num_seconds().max(0);
    if secs >= 3600 {
        format!("{}:{:02}:{:02}", secs / 3600, secs % 3600 / 60, secs % 60)
    } else {
        format!("{}:{:02}", secs / 60, secs % 60)
    }
}

/// `405 s` as `6:45` (whole seconds, rounded down).
pub fn format_duration(d: Duration) -> String {
    let secs = d.num_seconds().max(0);
    format!("{}:{:02}", secs / 60, secs % 60)
}

fn border(active: bool) -> Style {
    if active {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    }
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

pub fn render(app: &App, footer: &Footer, frame: &mut Frame) {
    let [main, foot] =
        Layout::vertical([Constraint::Min(8), Constraint::Length(2)]).areas(frame.area());
    let [left, right] = Layout::horizontal([
        Constraint::Length(left_width(main.width)),
        Constraint::Min(30),
    ])
    .areas(main);
    // Three areas, three questions: which pass? what is the satellite doing? what is the radio doing?
    let tracking_height = if app.tracking().is_some() { 4 } else { 0 };
    let [live_area, tracking_area, picker_area] = Layout::vertical([
        Constraint::Length(9),
        Constraint::Length(tracking_height),
        Constraint::Min(4),
    ])
    .areas(right);

    render_passes(app, left, frame);
    render_live(app, footer.now, live_area, frame);
    if tracking_height > 0 {
        render_tracking(app, footer.now, tracking_area, frame);
    }
    render_picker(app, picker_area, frame);
    render_footer(app, footer, foot, frame);
}

/// The pass list takes about 30% of the width: start, length and peak need little room.
fn left_width(total: u16) -> u16 {
    (total * 30 / 100).clamp(28, 36)
}

/// `ISS (ZARYA)` as `ISS`, `RS-44 & BREEZE-KM R/B` as `RS-44`: enough to tell satellites apart in
/// a list. The detail panel shows the whole name.
pub fn short_name(name: &str) -> &str {
    let end = [" (", " &"]
        .iter()
        .filter_map(|marker| name.find(marker))
        .min()
        .unwrap_or(name.len());
    name[..end].trim()
}

/// One displayed line of the pass list: a day heading, or a pass (by index into the rows).
enum Item {
    Day(String),
    Pass(usize),
}

fn list_items(rows: &[Row]) -> Vec<Item> {
    let mut items = Vec::new();
    let mut day = None;
    for (i, row) in rows.iter().enumerate() {
        let date = row.pass.aos.date_naive();
        if day != Some(date) {
            items.push(Item::Day(date.format("%a %d %b").to_string()));
            day = Some(date);
        }
        items.push(Item::Pass(i));
    }
    items
}

fn render_passes(app: &App, area: Rect, frame: &mut Frame) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Passes (UTC)")
        .border_style(border(!app.picker_focused()));
    let inner = block.inner(area);
    let rows = app.rows();
    let selected = app.selected_row().unwrap_or(0);
    let items = list_items(&rows);

    // Scroll so the selected pass (and the day heading above it, if there is room) is on screen.
    let height = (inner.height as usize).saturating_sub(1).max(1);
    let position = items
        .iter()
        .position(|item| matches!(item, Item::Pass(i) if *i == selected))
        .unwrap_or(0);
    let mut first = (position + 1).saturating_sub(height);
    if first == position && position > 0 && matches!(items[position - 1], Item::Day(_)) {
        first -= 1;
    }

    let mut lines = vec![Line::from(Span::styled(
        format!("  {:<5} {:>5} {:>4}  {}", "start", "len", "peak", "sat"),
        Style::default().fg(Color::DarkGray),
    ))];
    lines.extend(
        items
            .iter()
            .skip(first)
            .take(height)
            .map(|item| match item {
                Item::Day(heading) => Line::from(Span::styled(
                    format!(" {heading}"),
                    Style::default().fg(Color::DarkGray),
                )),
                Item::Pass(i) => {
                    let row = &rows[*i];
                    let cursor = if *i == selected { ">" } else { " " };
                    let flag = if row.short { "!" } else { " " };
                    let text = format!(
                        "{cursor} {} {:>5}{flag}{:>3.0}°  {}",
                        row.pass.aos.format("%H:%M"),
                        format_duration(row.duration()),
                        row.pass.max_elevation_deg,
                        short_name(&row.name),
                    );
                    let mut style = Style::default();
                    if row.short {
                        style = style.fg(Color::Yellow);
                    }
                    if *i == selected {
                        style = style.add_modifier(Modifier::BOLD);
                    }
                    Line::from(Span::styled(text, style))
                }
            }),
    );
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

/// The selected satellite and pass: who, when, where it is now, and what the radio would use.
fn render_live(app: &App, now: DateTime<Utc>, area: Rect, frame: &mut Frame) {
    let block = Block::default().borders(Borders::ALL).title("Live");
    let mut lines: Vec<Line> = Vec::new();
    if let (Some(row), Some(live)) = (app.selected_pass(), app.live(now)) {
        let pass = &row.pass;
        lines.push(Line::from(format!("{}  NORAD {}", row.name, row.norad_id)));
        lines.push(Line::from(format!(
            "{}-{}Z  {}{}  peak {:.0}° at {}Z",
            pass.aos.format("%H:%M:%S"),
            pass.los.format("%H:%M:%S"),
            format_duration(row.duration()),
            if row.short { "!" } else { "" },
            pass.max_elevation_deg,
            pass.tca.format("%H:%M:%S"),
        )));
        lines.push(Line::from(if now < pass.aos {
            format!("AOS in {}", format_countdown(pass.aos - now))
        } else if now <= pass.los {
            format!("LOS in {}", format_countdown(pass.los - now))
        } else {
            "pass over".to_string()
        }));
        lines.push(Line::from(format!(
            "Az {:.1}°  El {:.1}°",
            live.azimuth_deg, live.elevation_deg
        )));
        lines.push(Line::from(format!(
            "Range {:.0} km  Rate {:+.2} km/s",
            live.range_km, live.range_rate_km_s
        )));
        lines.extend(frequency_lines(&live));
    } else {
        lines.push(Line::from("no passes"));
    }
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

/// What the radio is doing: the tracked transmitter and when its satellite's pass starts and ends.
fn render_tracking(app: &App, now: DateTime<Utc>, area: Rect, frame: &mut Frame) {
    let block = Block::default().borders(Borders::ALL).title("Tracking");
    let Some(tracking) = app.tracking() else {
        return;
    };
    let green = Style::default().fg(Color::Green);
    let mut lines = vec![Line::from(Span::styled(
        format!(
            "TRACKING {}  nudge {:+} Hz",
            tracking.transmitter, tracking.nudge_hz
        ),
        green.add_modifier(Modifier::BOLD),
    ))];
    if let Some(countdown) = app.tracked_countdown(now) {
        let text = match countdown {
            Countdown::Before { aos_in, los_in } => format!(
                "pass: AOS in {}  LOS in {}",
                format_countdown(aos_in),
                format_countdown(los_in)
            ),
            Countdown::During { los_in } => format!("pass: LOS in {}", format_countdown(los_in)),
            Countdown::NoFurtherPass => "pass: no further pass listed".to_string(),
        };
        lines.push(Line::from(Span::styled(text, green)));
    }
    frame.render_widget(Paragraph::new(lines).block(block), area);
}
