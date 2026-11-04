//! Layout and rendering: the strip on top, the body below, then the status and hint lines.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{App, Focus};

/// Group a frequency in Hz as `14.074.000`.
fn format_hz(hz: u64) -> String {
    format!(
        "{}.{:03}.{:03}",
        hz / 1_000_000,
        hz / 1_000 % 1_000,
        hz % 1_000
    )
}

pub fn render(app: &App, frame: &mut Frame) {
    let [strip_area, body_area, status_area, hint_area] = Layout::vertical([
        Constraint::Length(4),
        Constraint::Min(3),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    render_strip(app, frame, strip_area);
    app.render_body(frame, body_area);

    if let Some(status) = app.status() {
        frame.render_widget(
            Paragraph::new(status).style(Style::default().fg(Color::Red)),
            status_area,
        );
    }
    frame.render_widget(
        Paragraph::new(hint(app)).style(Style::default().fg(Color::DarkGray)),
        hint_area,
    );
    app.render_overlay(frame, frame.area());
}

/// A placeholder until the rig-state strip ticket: frequency, mode and connection on row 1.
fn render_strip(app: &App, frame: &mut Frame, area: Rect) {
    let freq = app.frequency().map_or("-".to_string(), format_hz);
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
    let border = if app.focus() == Focus::Strip {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border)
        .title("rig-tui");
    frame.render_widget(
        Paragraph::new(format!("{freq}  {mode}  {connection}")).block(block),
        area,
    );
}

/// Shown in the body when the rig's mode has no panel.
pub(crate) fn render_no_panel(app: &App, frame: &mut Frame, area: Rect) {
    let text = match app.rig_mode() {
        Some(mode) => format!("No panel for {mode}"),
        None => "No mode reported yet".to_string(),
    };
    frame.render_widget(
        Paragraph::new(text)
            .style(Style::default().fg(Color::DarkGray))
            .block(Block::default().borders(Borders::ALL)),
        area,
    );
}

/// The keys that apply to the current focus.
fn hint(app: &App) -> &'static str {
    if app.has_overlay() {
        return "Esc close  Ctrl-C quit";
    }
    match app.focus() {
        Focus::Panel => "Enter send  F1-F6 soft keys  Tab/Esc strip  Ctrl-C quit",
        Focus::Strip if app.panel_active() => {
            "Tab input  F1-F6 soft keys  c config  q quit  Ctrl-C quit"
        }
        Focus::Strip => "c config  q quit  Ctrl-C quit",
    }
}
