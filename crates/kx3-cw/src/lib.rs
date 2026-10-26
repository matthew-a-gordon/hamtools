//! KX3 CW terminal logic: protocol parsing, rig session and UI state.

mod config;
pub use config::{Config, ConfigError, Kx3Config, config_path};

use std::collections::BTreeMap;
use std::fmt;

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

use rigctld_client::Client;

#[derive(Debug)]
pub enum SessionError {
    Rig(rigctld_client::Error),
    Tb(TbError),
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SessionError::Rig(e) => write!(f, "rig: {e}"),
            SessionError::Tb(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for SessionError {}

impl From<rigctld_client::Error> for SessionError {
    fn from(e: rigctld_client::Error) -> Self {
        SessionError::Rig(e)
    }
}

impl From<TbError> for SessionError {
    fn from(e: TbError) -> Self {
        SessionError::Tb(e)
    }
}

/// A KX3 reached through rigctld: reads decoded CW and sends CW text.
pub struct Session {
    client: Client,
}

impl Session {
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    /// Read (and thereby clear) the KX3's decoded-text buffer. Poll often: the buffer is small.
    pub async fn poll(&mut self) -> Result<TbReply, SessionError> {
        let reply = self.client.send_cmd("TB;").await?;
        Ok(parse_tb(&reply)?)
    }
}

impl Session {
    /// Send `text` as CW via the rig's keyer. Line breaks become spaces: rigctld commands are
    /// one line, so a raw newline would start a second command.
    pub async fn send(&mut self, text: &str) -> Result<(), SessionError> {
        let one_line = text
            .split(['\r', '\n'])
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        Ok(self.client.send_morse(&one_line).await?)
    }
}

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

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

/// What the event loop must do in response to a key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Send(String),
    Quit,
}

/// Terminal UI state: transcript plus the line being typed. No I/O.
#[derive(Debug, Default)]
pub struct App {
    lines: Vec<Line>,
    input: String,
    status: Option<String>,
    callsign: Option<String>,
    macros: BTreeMap<u8, String>,
}

impl App {
    /// An app that expands `{call}` in sent text to `callsign`.
    pub fn with_callsign(callsign: Option<String>) -> Self {
        Self {
            callsign,
            ..Self::default()
        }
    }

    /// Bind F1..F6 to canned messages (text is uppercase; `{CALL}` expands to the callsign).
    pub fn with_macros(mut self, macros: BTreeMap<u8, String>) -> Self {
        self.macros = macros;
        self
    }

    pub fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    pub fn new() -> Self {
        Self::default()
    }

    pub fn input(&self) -> &str {
        &self.input
    }

    pub fn lines(&self) -> &[Line] {
        &self.lines
    }

    /// Message shown beside the input line, e.g. the last rig error. `None` clears it.
    pub fn set_status(&mut self, status: Option<String>) {
        self.status = status;
    }

    /// Append text the KX3 decoded to the current RX line.
    pub fn on_rx(&mut self, text: &str) {
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

    /// Expand `{CALL}` in `text` (typed text is uppercased, so the placeholder is uppercase) and
    /// queue it for sending, echoing it into the transcript. Without a callsign, a text that
    /// needs one is refused with a status message.
    fn send(&mut self, text: &str) -> Option<Command> {
        let text = if text.contains("{CALL}") {
            let Some(call) = &self.callsign else {
                self.status = Some("no callsign set in the config file".to_string());
                return None;
            };
            text.replace("{CALL}", call)
        } else {
            text.to_string()
        };
        self.lines.push(Line {
            source: Source::Tx,
            text: text.clone(),
        });
        Some(Command::Send(text))
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<Command> {
        match key.code {
            KeyCode::Esc => Some(Command::Quit),
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(Command::Quit)
            }
            KeyCode::Char(c) => {
                self.input.extend(c.to_uppercase());
                None
            }
            KeyCode::Backspace => {
                self.input.pop();
                None
            }
            KeyCode::Enter if !self.input.trim().is_empty() => {
                let command = self.send(&self.input.clone())?;
                self.input.clear();
                Some(command)
            }
            KeyCode::F(n @ 1..=6) => match self.macros.get(&n).cloned() {
                Some(text) => self.send(&text),
                None => {
                    self.status = Some(format!("F{n} is not set"));
                    None
                }
            },
            _ => None,
        }
    }
}

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::{Line as TextLine, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

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

/// One `F<n> <text>` entry per bound macro, with the callsign filled in, sharing `width`
/// columns fairly: short entries stay whole and long ones give up the most.
fn macro_hints(app: &App, width: usize) -> String {
    const GAP: usize = 2;
    let entries: Vec<String> = app
        .macros
        .iter()
        .map(|(n, text)| {
            let text = match &app.callsign {
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

/// Draw the transcript (newest text at the bottom) above the input line.
pub fn render(app: &App, frame: &mut Frame) {
    let hint_height = u16::from(!app.macros.is_empty());
    let [transcript_area, hint_area, input_area] = Layout::vertical([
        Constraint::Min(3),
        Constraint::Length(hint_height),
        Constraint::Length(3),
    ])
    .areas(frame.area());

    if hint_height > 0 {
        let hints = macro_hints(app, hint_area.width as usize);
        frame.render_widget(
            Paragraph::new(hints).style(Style::default().fg(Color::DarkGray)),
            hint_area,
        );
    }

    let transcript_block = Block::default().borders(Borders::ALL).title("KX3 CW");
    let inner = transcript_block.inner(transcript_area);
    let mut rows: Vec<TextLine> = Vec::new();
    for line in app.lines() {
        let (prefix, color) = match line.source {
            Source::Rx => ("", Color::Reset),
            Source::Tx => ("", Color::Yellow),
        };
        for row in wrap(&format!("{prefix}{}", line.text), inner.width as usize) {
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

    let title = match &app.status {
        Some(status) => format!("Send (Enter) / quit (Esc) | {status}"),
        None => "Send (Enter) / quit (Esc)".to_string(),
    };
    let input = Paragraph::new(format!("> {}", app.input()))
        .block(Block::default().borders(Borders::ALL).title(title));
    frame.render_widget(input, input_area);
}
