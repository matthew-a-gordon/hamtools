//! Layout and rendering: the strip on top, the body below, then the status and hint lines.

use std::time::{SystemTime, UNIX_EPOCH};

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::activity::ActivityCtx;
use crate::app::{App, Entry, Focus};
use crate::layout::{self, Fit};
use crate::rig::Feature;
use crate::style::{dim, focus_border, red};

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
        frame.render_widget(Paragraph::new(status).style(red()), status_area);
    }
    frame.render_widget(Paragraph::new(hint(app)).style(dim()), hint_area);
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

fn row_one(app: &App) -> String {
    let freq = match app.entry() {
        Some(Entry::Freq(text)) => format!("freq> {text}_"),
        Some(Entry::SplitFreq(text)) => format!("split tx> {text}_"),
        None => app.frequency().map_or("-".to_string(), format_hz),
    };
    let mode = match (app.rig_mode(), app.passband()) {
        (Some(mode), Some(passband)) => format!("{mode} {passband}"),
        (Some(mode), None) => mode.to_string(),
        _ => "-".to_string(),
    };
    let connection = if app.connected() {
        "connected"
    } else {
        "disconnected"
    };
    format!(
        "{freq}  {mode}  step {}  {connection}",
        format_step(app.tuning_step())
    )
}

fn row_two(app: &App) -> String {
    let vfo = if app.is_unsupported(Feature::Vfo) {
        "VFO not supported".to_string()
    } else {
        app.vfo().map_or("VFO -".to_string(), |v| {
            format!("VFO {}", v.trim_start_matches("VFO"))
        })
    };
    let split = if app.is_unsupported(Feature::Split) {
        "split not supported".to_string()
    } else {
        match app.split() {
            None => "split -".to_string(),
            Some((false, _)) => "split off".to_string(),
            Some((true, tx_vfo)) => {
                let tx = tx_vfo.trim_start_matches("VFO");
                if app.is_unsupported(Feature::SplitFreq) {
                    format!("split {tx} (transmit frequency not supported)")
                } else {
                    let freq = app.transmit_frequency().map_or("-".to_string(), format_hz);
                    format!("split {tx} {freq}")
                }
            }
        }
    };
    let ptt = match app.ptt() {
        Some(true) => "TX",
        Some(false) => "RX",
        None => "-",
    };
    let smeter = if app.is_unsupported(Feature::SMeter) {
        "S-meter not supported".to_string()
    } else {
        app.strength().map_or("S-".to_string(), format_strength)
    };
    format!("{vfo}  {split}  {ptt}  {smeter}  {}", format_utc(app.utc()))
}

/// The two-row rig-state strip: what the rig last reported, never what was requested.
fn render_strip(app: &App, frame: &mut Frame, area: Rect) {
    let border = focus_border(app.focus() == Focus::Strip);
    let title = format!(
        "{} | {} | {} logged",
        app.activity_name(),
        app.rig_name(),
        app.logged_count()
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border)
        .title(title);
    frame.render_widget(
        Paragraph::new(format!("{}\n{}", row_one(app), row_two(app))).block(block),
        area,
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
        Paragraph::new(text)
            .style(dim())
            .block(Block::default().borders(Borders::ALL)),
        area,
    );
}

/// The keys that apply to the current focus.
/// One or two rows (split by `\n`) so every key fits in 80 columns.
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
