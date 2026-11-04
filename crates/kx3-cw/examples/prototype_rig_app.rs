//! PROTOTYPE (throwaway; wayfinder ticket "Prototype: rig app screen and key model").
//!
//! Three structurally different screens and key models for the general rig app, driven by a fake
//! in-memory rig. Nothing here touches rigctld, the config file or the Log.
//!
//!   cargo run -p kx3-cw --example prototype_rig_app
//!
//! Prototype-only keys (work in every variant): Ctrl-N / Ctrl-P switch variant, Ctrl-D toggles the
//! fake rig connection. Ctrl-C always quits.
//!
//!   A  Focus toggle   strip on top, panel below; Tab moves focus between them.
//!   B  Chords         CW typing is always live; app commands are Ctrl chords in CW mode.
//!   C  Front panel    dial and recent contacts left, panel right; F-key soft-key bar; arrows tune.

use std::time::{Duration, Instant};

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::{DefaultTerminal, Frame};

const VARIANTS: [&str; 3] = ["A: Focus toggle", "B: Chords", "C: Front panel"];
const STEPS: [u64; 5] = [10, 100, 1_000, 10_000, 100_000];
const MACROS: [&str; 6] = [
    "CQ CQ DE N0CALL N0CALL K",
    "N0CALL DE",
    "599 TU",
    "73 EE",
    "AGN?",
    "QRZ?",
];
const MACRO_LABELS: [&str; 6] = ["CQ", "DE", "599 TU", "73", "AGN?", "QRZ?"];
const CANNED_RX: &str = "CQ CQ DE T3ST T3ST K   T3ST DE N0*ALL 5NN 5NN BK   R TU 73 ";

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Lsb,
    Usb,
    Cw,
    Cwr,
    Am,
    Fm,
    Rtty,
}

impl Mode {
    const ALL: [Mode; 7] = [
        Mode::Lsb,
        Mode::Usb,
        Mode::Cw,
        Mode::Cwr,
        Mode::Am,
        Mode::Fm,
        Mode::Rtty,
    ];
    fn name(self) -> &'static str {
        match self {
            Mode::Lsb => "LSB",
            Mode::Usb => "USB",
            Mode::Cw => "CW",
            Mode::Cwr => "CWR",
            Mode::Am => "AM",
            Mode::Fm => "FM",
            Mode::Rtty => "RTTY",
        }
    }
    fn is_cw(self) -> bool {
        matches!(self, Mode::Cw | Mode::Cwr)
    }
}

struct Contact {
    time: String,
    call: String,
    grid: String,
    rst: String,
    freq: u64,
    mode: Mode,
}

struct LogForm {
    fields: [String; 6], // call, grid, name, rst sent, rst rcvd, comment
    idx: usize,
    freq: u64,
    mode: Mode,
}

struct ConfigForm {
    fields: [String; 4], // callsign, locator, address lookup, rig address
    idx: usize,
    note: String,
}

enum Modal {
    None,
    Log(LogForm),
    List,
    Config(ConfigForm),
}

struct Proto {
    variant: usize,
    freq: u64,
    mode: Mode,
    step: usize,
    connected: bool,
    freq_entry: Option<String>,
    focus_panel: bool,
    transcript: Vec<(bool, String)>,
    input: String,
    rx_pos: usize,
    last_rx: Instant,
    log: Vec<Contact>,
    modal: Modal,
    status: String,
    quit: bool,
    start: Instant,
}

fn fmt_freq(hz: u64) -> String {
    format!("{}.{:03}.{:03}", hz / 1_000_000, hz / 1000 % 1000, hz % 1000)
}

fn ctrl(key: &KeyEvent, c: char) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char(c)
}

impl Proto {
    fn new() -> Self {
        let sample = |time: &str, call: &str, grid: &str, rst: &str, freq, mode| Contact {
            time: time.into(),
            call: call.into(),
            grid: grid.into(),
            rst: rst.into(),
            freq,
            mode,
        };
        Self {
            variant: 0,
            freq: 14_060_000,
            mode: Mode::Cw,
            step: 1,
            connected: true,
            freq_entry: None,
            focus_panel: false,
            transcript: Vec::new(),
            input: String::new(),
            rx_pos: 0,
            last_rx: Instant::now(),
            log: vec![
                sample("1402Z", "T3ST", "EN82", "599", 14_060_000, Mode::Cw),
                sample("1415Z", "N0DX", "FN31", "59", 14_250_000, Mode::Usb),
                sample("1431Z", "T3ST/P", "EM79", "599", 7_030_000, Mode::Cw),
            ],
            modal: Modal::None,
            status: String::new(),
            quit: false,
            start: Instant::now(),
        }
    }

    /// Fake S-meter: 0..=15 where 9 is S9 and each step above is +10 dB (rough).
    fn smeter(&self) -> u8 {
        if !self.connected {
            return 0;
        }
        let t = self.start.elapsed().as_secs_f64();
        let level = if self.mode.is_cw() { 6.0 + 3.0 * (t * 0.9).sin() + 1.5 * (t * 3.1).sin() } else { 3.0 + 2.0 * (t * 0.5).sin() };
        level.clamp(0.0, 15.0) as u8
    }

    fn tick(&mut self) {
        if self.connected && self.mode.is_cw() && self.last_rx.elapsed() > Duration::from_millis(350)
        {
            self.last_rx = Instant::now();
            let chars: Vec<char> = CANNED_RX.chars().collect();
            for _ in 0..2 {
                let c = chars[self.rx_pos % chars.len()];
                self.rx_pos += 1;
                match self.transcript.last_mut() {
                    Some((false, text)) => text.push(c),
                    _ => self.transcript.push((false, c.to_string())),
                }
            }
        }
    }

    fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
        if !mode.is_cw() {
            self.focus_panel = false;
        }
    }

    fn cycle_mode(&mut self, dir: i32) {
        let i = Mode::ALL.iter().position(|m| *m == self.mode).unwrap_or(0) as i32;
        let n = Mode::ALL.len() as i32;
        self.set_mode(Mode::ALL[((i + dir).rem_euclid(n)) as usize]);
    }

    fn tune(&mut self, dir: i64) {
        if !self.connected {
            self.status = "disconnected: cannot tune".into();
            return;
        }
        self.freq = (self.freq as i64 + dir * STEPS[self.step] as i64).max(0) as u64;
    }

    fn change_step(&mut self, dir: i32) {
        self.step = (self.step as i32 + dir).clamp(0, STEPS.len() as i32 - 1) as usize;
    }

    fn open_log(&mut self) {
        if !self.connected {
            self.status = "log: needs a live rig connection".into();
            return;
        }
        let rst = if self.mode.is_cw() { "599" } else { "59" };
        self.modal = Modal::Log(LogForm {
            fields: [
                String::new(),
                String::new(),
                String::new(),
                rst.into(),
                rst.into(),
                String::new(),
            ],
            idx: 0,
            freq: self.freq,
            mode: self.mode,
        });
    }

    fn open_config(&mut self) {
        self.modal = Modal::Config(ConfigForm {
            fields: [
                "N0CALL".into(),
                "EN71DQ".into(),
                String::new(),
                "127.0.0.1:4532".into(),
            ],
            idx: 0,
            note: String::new(),
        });
    }

    fn start_freq_entry(&mut self, first: Option<char>) {
        if !self.connected {
            self.status = "disconnected: cannot set frequency".into();
            return;
        }
        self.freq_entry = Some(first.map(String::from).unwrap_or_default());
    }

    fn freq_entry_key(&mut self, key: KeyEvent) {
        let Some(buf) = self.freq_entry.as_mut() else {
            return;
        };
        match key.code {
            KeyCode::Esc => self.freq_entry = None,
            KeyCode::Backspace => {
                buf.pop();
            }
            KeyCode::Char(c) if c.is_ascii_digit() || c == '.' => buf.push(c),
            KeyCode::Enter => {
                match buf.parse::<f64>() {
                    Ok(mhz) if mhz > 0.0 => {
                        self.freq = (mhz * 1_000_000.0).round() as u64;
                        self.status = format!("rig reports {} MHz", fmt_freq(self.freq));
                    }
                    _ => self.status = format!("not a frequency: {buf}"),
                }
                self.freq_entry = None;
            }
            _ => {}
        }
    }

    fn send(&mut self, text: &str) {
        if !self.connected {
            self.status = "disconnected: cannot send".into();
            return;
        }
        self.transcript.push((true, text.to_string()));
    }

    /// Typing into the CW panel's input line, plus F1..F6 macros.
    fn text_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input.extend(c.to_uppercase())
            }
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Enter if !self.input.trim().is_empty() => {
                let text = std::mem::take(&mut self.input);
                self.send(&text);
            }
            KeyCode::F(n @ 1..=6) => self.send(MACROS[n as usize - 1]),
            _ => {}
        }
    }

    /// Returns true if a modal consumed the key.
    fn modal_key(&mut self, key: KeyEvent) -> bool {
        match &mut self.modal {
            Modal::None => return false,
            Modal::Log(form) => match key.code {
                KeyCode::Esc => self.modal = Modal::None,
                KeyCode::Tab => form.idx = (form.idx + 1) % 6,
                KeyCode::BackTab => form.idx = (form.idx + 5) % 6,
                KeyCode::Enter => {
                    if form.fields[0].trim().is_empty() {
                        self.status = "Call is required".into();
                    } else {
                        let c = Contact {
                            time: "now".into(),
                            call: form.fields[0].clone(),
                            grid: form.fields[1].clone(),
                            rst: form.fields[3].clone(),
                            freq: form.freq,
                            mode: form.mode,
                        };
                        self.status = format!("logged {}", c.call);
                        self.log.push(c);
                        self.modal = Modal::None;
                    }
                }
                KeyCode::Backspace => {
                    form.fields[form.idx].pop();
                }
                KeyCode::Char(c) => {
                    if form.idx < 2 {
                        form.fields[form.idx].extend(c.to_uppercase());
                    } else {
                        form.fields[form.idx].push(c);
                    }
                }
                _ => {}
            },
            Modal::List => match key.code {
                KeyCode::Esc | KeyCode::Char('L') => self.modal = Modal::None,
                KeyCode::Char('q') => self.quit = true,
                _ => {}
            },
            Modal::Config(form) => match key.code {
                KeyCode::Esc => self.modal = Modal::None,
                KeyCode::Tab => form.idx = (form.idx + 1) % 4,
                KeyCode::BackTab => form.idx = (form.idx + 3) % 4,
                KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.status = "config saved (prototype: nothing written)".into();
                    self.modal = Modal::None;
                }
                KeyCode::Enter if form.idx == 2 => {
                    form.note = if form.fields[2].trim().is_empty() {
                        "type an address first".into()
                    } else {
                        form.fields[1] = "EN71DQ".into();
                        form.fields[2].clear();
                        "looked up: locator set to EN71DQ; address discarded".into()
                    };
                }
                KeyCode::Backspace => {
                    form.fields[form.idx].pop();
                }
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    if form.idx == 1 || form.idx == 0 {
                        form.fields[form.idx].extend(c.to_uppercase());
                    } else {
                        form.fields[form.idx].push(c);
                    }
                }
                _ => {}
            },
        }
        true
    }

    fn on_key(&mut self, key: KeyEvent) {
        self.status.clear();
        if ctrl(&key, 'c') {
            self.quit = true;
            return;
        }
        if ctrl(&key, 'n') {
            self.variant = (self.variant + 1) % VARIANTS.len();
            return;
        }
        if ctrl(&key, 'p') {
            self.variant = (self.variant + VARIANTS.len() - 1) % VARIANTS.len();
            return;
        }
        if ctrl(&key, 'd') {
            self.connected = !self.connected;
            return;
        }
        if self.modal_key(key) {
            return;
        }
        if self.freq_entry.is_some() {
            self.freq_entry_key(key);
            return;
        }
        match self.variant {
            0 => self.key_a(key),
            1 => self.key_b(key),
            _ => self.key_c(key),
        }
    }

    /// Commands available whenever no text field has the keyboard.
    fn plain_command(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('l') => self.open_log(),
            KeyCode::Char('L') => self.modal = Modal::List,
            KeyCode::Char('c') => self.open_config(),
            KeyCode::Char('f') => self.start_freq_entry(None),
            KeyCode::Char(c) if c.is_ascii_digit() => self.start_freq_entry(Some(c)),
            KeyCode::Char('m') => self.cycle_mode(1),
            KeyCode::Char('M') => self.cycle_mode(-1),
            KeyCode::Left => self.tune(-1),
            KeyCode::Right => self.tune(1),
            KeyCode::Up => self.change_step(1),
            KeyCode::Down => self.change_step(-1),
            _ => {}
        }
    }

    // Variant A: Tab moves focus between the strip (commands) and the CW panel (typing).
    fn key_a(&mut self, key: KeyEvent) {
        if self.mode.is_cw() && matches!(key.code, KeyCode::F(1..=6)) {
            self.text_key(key);
            return;
        }
        if self.mode.is_cw() && self.focus_panel {
            match key.code {
                KeyCode::Esc | KeyCode::Tab => self.focus_panel = false,
                _ => self.text_key(key),
            }
        } else if key.code == KeyCode::Tab && self.mode.is_cw() {
            self.focus_panel = true;
        } else {
            self.plain_command(key);
        }
    }

    // Variant B: in CW mode typing is always live and commands are Ctrl chords; elsewhere the
    // plain keys work because nothing captures text.
    fn key_b(&mut self, key: KeyEvent) {
        if ctrl(&key, 'q') {
            self.quit = true;
        } else if ctrl(&key, 'f') {
            self.start_freq_entry(None);
        } else if ctrl(&key, 'e') {
            self.cycle_mode(1);
        } else if ctrl(&key, 'l') {
            self.open_log();
        } else if ctrl(&key, 'b') {
            self.modal = Modal::List;
        } else if ctrl(&key, 'o') {
            self.open_config();
        } else if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Left {
            self.tune(-1);
        } else if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Right {
            self.tune(1);
        } else if self.mode.is_cw() {
            self.text_key(key);
        } else {
            self.plain_command(key);
        }
    }

    // Variant C: F7..F12 are the app's soft keys everywhere; arrows always tune (the CW input
    // has no cursor movement), so typing and tuning never conflict.
    fn key_c(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::F(7) => self.start_freq_entry(None),
            KeyCode::F(8) => self.cycle_mode(1),
            KeyCode::F(9) => self.open_log(),
            KeyCode::F(10) => self.modal = Modal::List,
            KeyCode::F(11) => self.open_config(),
            KeyCode::F(12) => self.quit = true,
            KeyCode::Left => self.tune(-1),
            KeyCode::Right => self.tune(1),
            KeyCode::Up => self.change_step(1),
            KeyCode::Down => self.change_step(-1),
            _ if self.mode.is_cw() => self.text_key(key),
            KeyCode::Char('q') => self.quit = true,
            _ => self.plain_command(key),
        }
    }
}

// ---------------------------------------------------------------- drawing

fn dim() -> Style {
    Style::default().fg(Color::DarkGray)
}

fn strip_line(p: &Proto) -> Line<'static> {
    let conn = if p.connected {
        Span::styled("● connected", Style::default().fg(Color::Green))
    } else {
        Span::styled("○ disconnected (retrying)", Style::default().fg(Color::Red))
    };
    let freq = match &p.freq_entry {
        Some(buf) => Span::styled(
            format!("{buf}_ MHz"),
            Style::default().add_modifier(Modifier::REVERSED),
        ),
        None => Span::styled(
            format!("{} MHz", fmt_freq(p.freq)),
            Style::default().add_modifier(Modifier::BOLD),
        ),
    };
    Line::from(vec![
        freq,
        Span::raw("   "),
        Span::styled(p.mode.name(), Style::default().fg(Color::Yellow)),
        Span::raw(format!("   step {} Hz   ", STEPS[p.step])),
        conn,
    ])
}

fn cw_panel(f: &mut Frame, area: Rect, p: &Proto, focused: bool, show_cursor: bool) {
    let border = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border)
        .title(format!(" CW · {} ", if p.connected { "decoding" } else { "no rig" }));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let [text, hints, input] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(inner);
    let rows: Vec<Line> = p
        .transcript
        .iter()
        .map(|(tx, t)| {
            Line::styled(
                t.clone(),
                Style::default().fg(if *tx { Color::Yellow } else { Color::Reset }),
            )
        })
        .collect();
    let hidden = rows.len().saturating_sub(text.height as usize);
    f.render_widget(Paragraph::new(rows[hidden..].to_vec()), text);
    let softkeys: Vec<Span> = MACRO_LABELS
        .iter()
        .enumerate()
        .flat_map(|(i, label)| {
            [
                Span::styled(format!(" F{} ", i + 1), Style::default().add_modifier(Modifier::REVERSED)),
                Span::raw(format!("{label}  ")),
            ]
        })
        .collect();
    f.render_widget(Paragraph::new(Line::from(softkeys)), hints);
    let cursor = if show_cursor { "_" } else { "" };
    f.render_widget(Paragraph::new(format!("> {}{cursor}", p.input)), input);
}

fn no_panel(f: &mut Frame, area: Rect, p: &Proto) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(dim())
        .title(format!(" no panel for {} ", p.mode.name()));
    f.render_widget(
        Paragraph::new("Mode panels appear here. Only CW has one so far.")
            .style(dim())
            .block(block),
        area,
    );
}

fn protobar(f: &mut Frame, area: Rect, p: &Proto) {
    let line = format!(
        " PROTOTYPE · {} · Ctrl-N/P variant · Ctrl-D rig {} {}",
        VARIANTS[p.variant],
        if p.connected { "on" } else { "off" },
        if p.status.is_empty() {
            String::new()
        } else {
            format!("· {}", p.status)
        }
    );
    f.render_widget(
        Paragraph::new(line).style(Style::default().bg(Color::Yellow).fg(Color::Black)),
        area,
    );
}

fn draw(f: &mut Frame, p: &Proto) {
    match p.variant {
        0 => draw_a(f, p),
        1 => draw_b(f, p),
        _ => draw_c(f, p),
    }
    match &p.modal {
        Modal::None => {}
        Modal::Log(form) => draw_log(f, form, &p.log),
        Modal::List => draw_list(f, &p.log),
        Modal::Config(form) => draw_config(f, form),
    }
}

fn draw_a(f: &mut Frame, p: &Proto) {
    let [strip, body, hints, bar] = Layout::vertical([
        Constraint::Length(4),
        Constraint::Min(5),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(f.area());
    let focus_note = if p.mode.is_cw() {
        if p.focus_panel { "focus: PANEL" } else { "focus: STRIP" }
    } else {
        "focus: STRIP"
    };
    let focus_strip = !(p.mode.is_cw() && p.focus_panel);
    f.render_widget(
        Paragraph::new(vec![strip_line(p), rig_state_line(p)]).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(if focus_strip {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default()
                })
                .title(format!(" rig-tui · {focus_note} ")),
        ),
        strip,
    );
    if p.mode.is_cw() {
        cw_panel(f, body, p, p.focus_panel, p.focus_panel);
    } else {
        no_panel(f, body, p);
    }
    let hint = if focus_strip {
        "digits/f freq  ←→ tune  ↑↓ step  m/M mode  l log  L list  c config  Tab→panel  q quit"
    } else {
        "typing → CW · Enter send · F1-F6 macros · Tab/Esc → strip"
    };
    f.render_widget(Paragraph::new(hint).style(dim()), hints);
    protobar(f, bar, p);
}

fn draw_b(f: &mut Frame, p: &Proto) {
    let [strip, body, hints, bar] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(5),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(f.area());
    f.render_widget(
        Paragraph::new(strip_line(p)).style(Style::default().add_modifier(Modifier::REVERSED)),
        strip,
    );
    if p.mode.is_cw() {
        cw_panel(f, body, p, true, true);
    } else {
        no_panel(f, body, p);
    }
    let hint = if p.mode.is_cw() {
        "^F freq  ^←→ tune  ^E mode  ^L log  ^B list  ^O config  ^Q quit   (everything else types)"
    } else {
        "plain keys: digits/f freq  ←→ tune  m mode  l log  L list  c config  q quit   (^chords too)"
    };
    f.render_widget(Paragraph::new(hint).style(dim()), hints);
    protobar(f, bar, p);
}

fn draw_c(f: &mut Frame, p: &Proto) {
    let [main, softkeys, bar] = Layout::vertical([
        Constraint::Min(8),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(f.area());
    let [left, right] =
        Layout::horizontal([Constraint::Length(34), Constraint::Min(20)]).areas(main);
    let [dial, recent] = Layout::vertical([Constraint::Length(8), Constraint::Min(3)]).areas(left);

    let big = match &p.freq_entry {
        Some(buf) => format!("{buf}_"),
        None => fmt_freq(p.freq),
    };
    let modes: Vec<Span> = Mode::ALL
        .iter()
        .flat_map(|m| {
            let style = if *m == p.mode {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                dim()
            };
            [Span::styled(m.name(), style), Span::raw(" ")]
        })
        .collect();
    let conn = if p.connected {
        Span::styled("● connected", Style::default().fg(Color::Green))
    } else {
        Span::styled("○ disconnected", Style::default().fg(Color::Red))
    };
    f.render_widget(
        Paragraph::new(vec![
            Line::from(""),
            Line::styled(
                format!("{big}"),
                Style::default().add_modifier(Modifier::BOLD).fg(Color::White),
            ),
            Line::styled("MHz", dim()),
            Line::from(""),
            Line::from(modes),
            Line::from(format!("step {} Hz  (↑↓)", STEPS[p.step])),
            Line::from(conn),
        ])
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL).title(" rig-tui ")),
        dial,
    );
    let last: Vec<Line> = p
        .log
        .iter()
        .rev()
        .take(3)
        .rev()
        .map(|c| Line::from(format!("{} {} {}", c.time, c.call, c.mode.name())))
        .collect();
    f.render_widget(
        Paragraph::new(last).block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" log ({}) ", p.log.len())),
        ),
        recent,
    );
    if p.mode.is_cw() {
        cw_panel(f, right, p, true, true);
    } else {
        no_panel(f, right, p);
    }
    let keys = [
        ("F7", "Freq"),
        ("F8", "Mode"),
        ("F9", "Log"),
        ("F10", "List"),
        ("F11", "Config"),
        ("F12", "Quit"),
    ];
    let spans: Vec<Span> = keys
        .iter()
        .flat_map(|(k, label)| {
            [
                Span::styled(
                    format!(" {k} "),
                    Style::default().add_modifier(Modifier::REVERSED),
                ),
                Span::raw(format!("{label}  ")),
            ]
        })
        .collect();
    f.render_widget(Paragraph::new(Line::from(spans)), softkeys);
    protobar(f, bar, p);
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}

fn field_line(label: &str, value: &str, active: bool) -> Line<'static> {
    let style = if active {
        Style::default().add_modifier(Modifier::REVERSED)
    } else {
        Style::default()
    };
    Line::from(vec![
        Span::raw(format!("{label:<10}")),
        Span::styled(format!("{value}{}", if active { "_" } else { "" }), style),
    ])
}

fn draw_log(f: &mut Frame, form: &LogForm, _log: &[Contact]) {
    let area = centered(f.area(), 60, 14);
    f.render_widget(Clear, area);
    let names = ["Call", "Grid", "Name", "RST sent", "RST rcvd", "Comment"];
    let mut lines = vec![
        Line::styled(
            format!(
                "snapshot: {} MHz {} · opened 0:07 ago",
                fmt_freq(form.freq),
                form.mode.name()
            ),
            dim(),
        ),
        Line::styled("rig now: tracking (form keeps running underneath)", dim()),
        Line::from(""),
    ];
    for (i, n) in names.iter().enumerate() {
        lines.push(field_line(n, &form.fields[i], i == form.idx));
    }
    lines.push(Line::from(""));
    lines.push(Line::styled("Tab fields · Enter save · Esc cancel", dim()));
    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Log contact · my locator EN71DQ "),
        ),
        area,
    );
}

fn draw_list(f: &mut Frame, log: &[Contact]) {
    let area = centered(f.area(), 64, 12);
    f.render_widget(Clear, area);
    let mut lines: Vec<Line> = log
        .iter()
        .map(|c| {
            Line::from(format!(
                "{:<6} {:<8} {:<5} {:<4} {} {}",
                c.time,
                c.call,
                c.grid,
                c.rst,
                fmt_freq(c.freq),
                c.mode.name()
            ))
        })
        .collect();
    lines.push(Line::from(""));
    lines.push(Line::styled("L/Esc close · q quit", dim()));
    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" This session's contacts "),
        ),
        area,
    );
}

fn draw_config(f: &mut Frame, form: &ConfigForm) {
    let area = centered(f.area(), 66, 16);
    f.render_widget(Clear, area);
    let names = ["Callsign", "Locator", "Address", "Rig addr"];
    let mut lines = Vec::new();
    for (i, n) in names.iter().enumerate() {
        lines.push(field_line(n, &form.fields[i], i == form.idx));
    }
    lines.push(Line::from(""));
    lines.push(Line::styled(
        "Address: Enter looks it up online (sent to a third party, never saved);",
        dim(),
    ));
    lines.push(Line::styled(
        "only the 6-character locator is kept. You can also type the locator.",
        dim(),
    ));
    if !form.note.is_empty() {
        lines.push(Line::styled(form.note.clone(), Style::default().fg(Color::Green)));
    }
    lines.push(Line::from(""));
    lines.push(Line::styled(
        "file: ~/.config/hamtools/config.toml (writable) · hand edits are fine",
        dim(),
    ));
    lines.push(Line::styled("Tab fields · Ctrl-S save · Esc cancel", dim()));
    f.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(" Config ")),
        area,
    );
}

fn run(terminal: &mut DefaultTerminal) -> std::io::Result<()> {
    let mut p = Proto::new();
    while !p.quit {
        p.tick();
        terminal.draw(|f| draw(f, &p))?;
        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            p.on_key(key);
        }
    }
    Ok(())
}

fn main() -> std::io::Result<()> {
    let mut terminal = ratatui::init();
    let result = run(&mut terminal);
    ratatui::restore();
    result
}

/// Second strip row for variant A: VFO, split, RX/TX, S-meter and a UTC clock.
fn rig_state_line(p: &Proto) -> Line<'static> {
    let s = p.smeter();
    let label = if s <= 9 {
        format!("S{s}")
    } else {
        format!("S9+{}", (s - 9) * 10)
    };
    let bar: String = (0..15).map(|i| if i < s { '▮' } else { '▯' }).collect();
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
        % 86_400;
    let clock = format!("{:02}:{:02}:{:02}Z", secs / 3600, secs / 60 % 60, secs % 60);
    let meter_style = Style::default().fg(if s > 9 { Color::Red } else { Color::Green });
    Line::from(vec![
        Span::raw("VFO A   split off   "),
        Span::styled("RX", Style::default().fg(Color::Green)),
        Span::raw("   "),
        Span::styled(bar, meter_style),
        Span::raw(format!(" {label:<6}")),
        Span::raw("   "),
        Span::styled(clock, dim()),
    ])
}
