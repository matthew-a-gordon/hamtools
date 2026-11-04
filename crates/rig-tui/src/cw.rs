//! The CW Mode panel: decoded transcript, input line and the Soft key bar.
//!
//! Applies to CW and CWR. It polls the KX3's `TB;` decode buffer each tick while active and is
//! the only thing in the app that can key the transmitter, through [`PanelCommand::Send`].

use std::collections::BTreeMap;
use std::fmt;

use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line as TextLine, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::panel::{LogMode, ModePanel, PanelCommand, PanelCtx};
use crate::rig::RigMode;

/// Decoded form of a KX3 `TB;` reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TbReply {
    /// Characters still waiting to be sent as CW (the KX3 caps this at 9).
    pub tx_pending: u8,
    /// Characters the KX3 decoded since the last `TB;`.
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TbError {
    pub reply: String,
    pub reason: &'static str,
}

impl fmt::Display for TbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "bad TB reply {:?}: {}", self.reply, self.reason)
    }
}

impl std::error::Error for TbError {}

/// Parse a `TBtrrs;` reply: `t` pending TX chars, `rr` RX char count, `s` the RX text.
/// The count, not the trailing semicolon, delimits the text, since text may contain `;`.
pub fn parse_tb(reply: &str) -> Result<TbReply, TbError> {
    let err = |reason| TbError {
        reply: reply.to_string(),
        reason,
    };
    let body = reply
        .strip_prefix("TB")
        .ok_or_else(|| err("missing TB prefix"))?;
    let body = body
        .strip_suffix(';')
        .ok_or_else(|| err("missing terminating semicolon"))?;
    let (counts, text) = body.split_at_checked(3).ok_or_else(|| err("too short"))?;
    let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    if !digits(counts) {
        return Err(err("non-numeric counts"));
    }
    let tx_pending = counts[..1].parse().map_err(|_| err("bad transmit count"))?;
    let rx_count: usize = counts[1..].parse().map_err(|_| err("bad receive count"))?;
    if text.chars().count() != rx_count {
        return Err(err("receive count does not match text length"));
    }
    Ok(TbReply {
        tx_pending,
        text: text.to_string(),
    })
}

/// Where a transcript line came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Decoded by the KX3 from the air.
    Rx,
    /// Typed here and sent.
    Tx,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub source: Source,
    pub text: String,
}

const TB: &str = "TB;";

#[derive(Debug, Default)]
pub struct CwPanel {
    lines: Vec<Line>,
    input: String,
}

impl CwPanel {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append text the KX3 decoded to the current RX line.
    fn on_rx(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        match self.lines.last_mut() {
            Some(line) if line.source == Source::Rx => line.text.push_str(text),
            _ => self.lines.push(Line {
                source: Source::Rx,
                text: text.to_string(),
            }),
        }
    }

    /// Expand `{CALL}` in `text` and queue it for sending, echoing it into the transcript.
    /// Refused with a status message while the rig is disconnected, or when the text needs a
    /// callsign and none is configured.
    fn send(&mut self, text: &str, ctx: &PanelCtx) -> Vec<PanelCommand> {
        if !ctx.connected {
            return vec![PanelCommand::Status(
                "rig disconnected: cannot send".to_string(),
            )];
        }
        let text = match (text.contains("{CALL}"), ctx.callsign) {
            (true, None) => {
                return vec![PanelCommand::Status(
                    "no callsign set in the config file".to_string(),
                )];
            }
            (true, Some(call)) => text.replace("{CALL}", call),
            (false, _) => text.to_string(),
        };
        self.lines.push(Line {
            source: Source::Tx,
            text: text.clone(),
        });
        vec![PanelCommand::Send(text)]
    }
}

impl ModePanel for CwPanel {
    fn applies(&self, mode: &RigMode) -> bool {
        matches!(mode, RigMode::Cw | RigMode::Cwr)
    }

    fn captures_text(&self) -> bool {
        true
    }

    fn handle_key(&mut self, key: KeyEvent, ctx: &PanelCtx) -> Vec<PanelCommand> {
        match key.code {
            KeyCode::F(n @ 1..=6) => match ctx.soft_keys.get(&n) {
                Some(text) => {
                    let text = text.clone();
                    self.send(&text, ctx)
                }
                None => vec![PanelCommand::Status(format!("F{n} is not set"))],
            },
            // Only the input line takes text; other keys belong to the app.
            _ if !ctx.focused => Vec::new(),
            KeyCode::Char(c) => {
                self.input.extend(c.to_uppercase());
                Vec::new()
            }
            KeyCode::Backspace => {
                self.input.pop();
                Vec::new()
            }
            KeyCode::Enter if !self.input.trim().is_empty() => {
                let commands = self.send(&self.input.clone(), ctx);
                if commands.iter().any(|c| matches!(c, PanelCommand::Send(_))) {
                    self.input.clear();
                }
                commands
            }
            _ => Vec::new(),
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect, ctx: &PanelCtx) {
        let [transcript_area, input_area, bar_area] = Layout::vertical([
            Constraint::Min(3),
            Constraint::Length(3),
            Constraint::Length(u16::from(!ctx.soft_keys.is_empty())),
        ])
        .areas(area);

        let transcript_block = Block::default().borders(Borders::ALL).title("CW");
        let inner = transcript_block.inner(transcript_area);
        let mut rows: Vec<TextLine> = Vec::new();
        for line in &self.lines {
            let color = match line.source {
                Source::Rx => Color::Reset,
                Source::Tx => Color::Yellow,
            };
            for row in wrap(&line.text, inner.width as usize) {
                rows.push(TextLine::from(Span::styled(
                    row,
                    Style::default().fg(color),
                )));
            }
        }
        let hidden = rows.len().saturating_sub(inner.height as usize);
        frame.render_widget(
            Paragraph::new(rows.split_off(hidden)).block(transcript_block),
            transcript_area,
        );

        let border = if ctx.focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default()
        };
        let input = Paragraph::new(format!("> {}", self.input)).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border)
                .title("Send (Enter)"),
        );
        frame.render_widget(input, input_area);

        if bar_area.height > 0 {
            let bar = soft_key_bar(ctx.soft_keys, ctx.callsign, bar_area.width as usize);
            frame.render_widget(
                Paragraph::new(bar).style(Style::default().add_modifier(Modifier::REVERSED)),
                bar_area,
            );
        }
    }

    fn due_requests(&mut self) -> Vec<String> {
        vec![TB.to_string()]
    }

    fn on_reply(&mut self, request: &str, reply: Result<String, String>) -> Result<(), String> {
        if request != TB {
            return Ok(());
        }
        let polled = parse_tb(&reply?).map_err(|e| e.to_string())?;
        self.on_rx(&polled.text);
        Ok(())
    }

    fn logbook_mode(&self, mode: &RigMode) -> Option<LogMode> {
        self.applies(mode).then(|| LogMode {
            mode: "CW".to_string(),
            submode: None,
            default_rst: Some("599".to_string()),
        })
    }
}

/// Split `text` into rows of at most `width` characters.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return vec![String::new()];
    }
    chars
        .chunks(width.max(1))
        .map(|chunk| chunk.iter().collect())
        .collect()
}

/// Cut `text` to at most `max` characters, ending in `…` when it was shortened.
fn shorten(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let keep: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{keep}…")
}

/// One `F<n> <text>` entry per bound Soft key, with the callsign filled in, sharing `width`
/// columns fairly: short entries stay whole and long ones give up the most.
fn soft_key_bar(soft_keys: &BTreeMap<u8, String>, callsign: Option<&str>, width: usize) -> String {
    const GAP: usize = 2;
    let entries: Vec<String> = soft_keys
        .iter()
        .map(|(n, text)| {
            let text = match callsign {
                Some(call) => text.replace("{CALL}", call),
                None => text.clone(),
            };
            format!("F{n} {text}")
        })
        .collect();
    let mut by_length: Vec<usize> = (0..entries.len()).collect();
    by_length.sort_by_key(|&i| entries[i].chars().count());
    let mut remaining = width.saturating_sub(GAP * entries.len().saturating_sub(1));
    let mut budget = vec![0; entries.len()];
    for (rank, &i) in by_length.iter().enumerate() {
        let share = remaining / (entries.len() - rank);
        budget[i] = entries[i].chars().count().min(share);
        remaining -= budget[i];
    }
    entries
        .iter()
        .zip(&budget)
        .map(|(entry, &max)| shorten(entry, max))
        .collect::<Vec<_>>()
        .join(&" ".repeat(GAP))
}
