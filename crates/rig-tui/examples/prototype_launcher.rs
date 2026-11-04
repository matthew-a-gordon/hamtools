//! PROTOTYPE (throwaway; wayfinder ticket "Prototype: launcher home view and navigation").
//!
//! Three structurally different ways to move between Activities (and plans), over a fake in-memory
//! rig. Nothing here touches rigctld, the config file, the Log or the network. All data is made up.
//!
//!   cargo run -p rig-tui --example prototype_launcher
//!   cargo run -p rig-tui --example prototype_launcher -- --dump 80x24     (headless renders)
//!
//! Prototype-only keys (every variant): Ctrl-N / Ctrl-P switch variant. Ctrl-C quits.
//! Overlays reachable from anywhere (every variant): Ctrl-O config, Ctrl-L log form,
//! Ctrl-B bandplan, Ctrl-R rig switcher. Esc closes an overlay.
//!
//!   A  Launcher   a Home screen you drill down from; Esc returns Home. Up/Down, Enter, q quits.
//!   B  Rail       no Home: Activities and plans are always visible (left rail when wide, tab bar
//!                 when narrow). Alt-1..8 or Alt-Left/Right switch.
//!   C  Palette    the body is always an Activity (starts in General); Ctrl-G opens a type-to-filter
//!                 palette over it.

use std::time::{Duration, Instant};

use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::{Frame, Terminal};

const VARIANTS: [&str; 3] = ["A: Launcher", "B: Rail", "C: Palette"];

struct Entry {
    name: &'static str,
    short: &'static str,
    needs_sat: bool,
}

const ENTRIES: [Entry; 6] = [
    Entry { name: "General", short: "Gen", needs_sat: false },
    Entry { name: "Satellite", short: "Sat", needs_sat: true },
    Entry { name: "SOTA Activator", short: "SOTA-A", needs_sat: false },
    Entry { name: "SOTA Chaser", short: "SOTA-C", needs_sat: false },
    Entry { name: "POTA Activator", short: "POTA-A", needs_sat: false },
    Entry { name: "POTA Hunter", short: "POTA-H", needs_sat: false },
];

struct Plan {
    label: &'static str,
    entry: usize,
}

// Made-up references.
const PLANS: [Plan; 2] = [
    Plan { label: "Sat 09:30  SOTA W0X/XX-001", entry: 2 },
    Plan { label: "Sun 14:00  POTA K-0000", entry: 4 },
];

const ITEMS: usize = ENTRIES.len() + PLANS.len();

fn item_entry(i: usize) -> usize {
    if i < ENTRIES.len() {
        i
    } else {
        PLANS[i - ENTRIES.len()].entry
    }
}

fn item_label(i: usize) -> String {
    if i < ENTRIES.len() {
        ENTRIES[i].name.to_string()
    } else {
        format!("Plan: {}", PLANS[i - ENTRIES.len()].label)
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Rig {
    Kx3,
    Ft847,
}

#[derive(Clone, Copy, PartialEq)]
enum Modal {
    None,
    Config,
    Log,
    Band,
    Rig,
}

struct Proto {
    variant: usize,
    home: bool,
    cur: usize,
    sel: usize,
    rig: Rig,
    modal: Modal,
    palette: Option<String>,
    pal_sel: usize,
    status: String,
    quit: bool,
    start: Instant,
}

impl Proto {
    fn new(variant: usize) -> Self {
        Proto {
            variant,
            home: variant == 0,
            cur: 0,
            sel: 0,
            rig: Rig::Kx3,
            modal: Modal::None,
            palette: None,
            pal_sel: 0,
            status: String::new(),
            quit: false,
            start: Instant::now(),
        }
    }

    fn enabled(&self, entry: usize) -> bool {
        !(ENTRIES[entry].needs_sat && self.rig == Rig::Kx3)
    }

    fn launch(&mut self, item: usize) {
        let e = item_entry(item);
        if !self.enabled(e) {
            self.status = format!("{} needs the FT-847 (sat mode)", ENTRIES[e].name);
            return;
        }
        self.cur = e;
        self.sel = item;
        self.home = false;
        self.palette = None;
        self.status.clear();
    }

    fn set_rig(&mut self, rig: Rig) {
        self.rig = rig;
        if !self.enabled(self.cur) {
            self.cur = 0;
            self.sel = 0;
            self.home = self.variant == 0;
            self.status = "Rig switched; Satellite stops, back to General".into();
        } else {
            self.status = "Rig switched".into();
        }
    }

    fn filtered(&self) -> Vec<usize> {
        let f = self.palette.clone().unwrap_or_default().to_lowercase();
        (0..ITEMS)
            .filter(|&i| item_label(i).to_lowercase().contains(&f))
            .collect()
    }

    fn on_key(&mut self, k: KeyEvent) {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let alt = k.modifiers.contains(KeyModifiers::ALT);
        if ctrl {
            let m = match k.code {
                KeyCode::Char('c') => {
                    self.quit = true;
                    return;
                }
                KeyCode::Char('n') => {
                    self.variant = (self.variant + 1) % 3;
                    self.palette = None;
                    return;
                }
                KeyCode::Char('p') => {
                    self.variant = (self.variant + 2) % 3;
                    self.palette = None;
                    return;
                }
                KeyCode::Char('o') => Modal::Config,
                KeyCode::Char('l') => Modal::Log,
                KeyCode::Char('b') => Modal::Band,
                KeyCode::Char('r') => Modal::Rig,
                KeyCode::Char('g') if self.variant == 2 => {
                    self.modal = Modal::None;
                    self.palette = Some(String::new());
                    self.pal_sel = 0;
                    return;
                }
                _ => Modal::None,
            };
            if m != Modal::None {
                self.palette = None;
                self.modal = m;
                return;
            }
        }
        if self.modal != Modal::None {
            match k.code {
                KeyCode::Esc => self.modal = Modal::None,
                KeyCode::Enter | KeyCode::Left | KeyCode::Right if self.modal == Modal::Rig => {
                    let r = if self.rig == Rig::Kx3 { Rig::Ft847 } else { Rig::Kx3 };
                    self.set_rig(r);
                }
                _ => {}
            }
            return;
        }
        if self.palette.is_some() {
            let n = self.filtered().len();
            match k.code {
                KeyCode::Esc => self.palette = None,
                KeyCode::Char(c) => {
                    self.palette.as_mut().unwrap().push(c);
                    self.pal_sel = 0;
                }
                KeyCode::Backspace => {
                    self.palette.as_mut().unwrap().pop();
                    self.pal_sel = 0;
                }
                KeyCode::Down if n > 0 => self.pal_sel = (self.pal_sel + 1) % n,
                KeyCode::Up if n > 0 => self.pal_sel = (self.pal_sel + n - 1) % n,
                KeyCode::Enter => {
                    if let Some(&i) = self.filtered().get(self.pal_sel) {
                        self.launch(i);
                    }
                }
                _ => {}
            }
            return;
        }
        match self.variant {
            0 => match k.code {
                KeyCode::Esc | KeyCode::Home if !self.home => self.home = true,
                KeyCode::Char('q') if self.home => self.quit = true,
                KeyCode::Down if self.home => self.sel = (self.sel + 1) % ITEMS,
                KeyCode::Up if self.home => self.sel = (self.sel + ITEMS - 1) % ITEMS,
                KeyCode::Enter if self.home => self.launch(self.sel),
                _ => {}
            },
            1 => match k.code {
                KeyCode::Char(c @ '1'..='8') if alt => self.launch(c as usize - '1' as usize),
                KeyCode::Right if alt => self.launch((self.sel + 1) % ITEMS),
                KeyCode::Left if alt => self.launch((self.sel + ITEMS - 1) % ITEMS),
                KeyCode::Char('q') => self.quit = true,
                _ => {}
            },
            _ => {
                if let KeyCode::Char('q') = k.code {
                    self.quit = true;
                }
            }
        }
    }

    // ---- rendering ----

    fn render(&self, f: &mut Frame) {
        let area = f.area();
        let [strip, body, hint] = Layout::vertical([
            Constraint::Length(4),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .areas(area);
        self.render_strip(f, strip);
        match self.variant {
            0 => {
                if self.home {
                    self.render_home(f, body);
                } else {
                    self.render_activity(f, body);
                }
            }
            1 => {
                if area.width >= 100 {
                    let [rail, main] =
                        Layout::horizontal([Constraint::Length(34), Constraint::Min(1)]).areas(body);
                    self.render_rail(f, rail);
                    self.render_activity(f, main);
                } else {
                    let [tabs, main] =
                        Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(body);
                    self.render_tabs(f, tabs);
                    self.render_activity(f, main);
                }
            }
            _ => self.render_activity(f, body),
        }
        self.render_hint(f, hint);
        if let Some(filter) = &self.palette {
            self.render_palette(f, area, filter);
        }
        if self.modal != Modal::None {
            self.render_modal(f, area);
        }
    }

    fn render_strip(&self, f: &mut Frame, area: Rect) {
        let (rig, freq, mode) = match self.rig {
            Rig::Kx3 => ("KX3 :4532", "14.060.00", "CW 400 Hz"),
            Rig::Ft847 => ("FT-847 :4533", "145.900.00", "FM"),
        };
        let secs = 14 * 3600 + 7 * 60 + self.start.elapsed().as_secs();
        let clock = format!("{:02}:{:02}:{:02}Z", secs / 3600 % 24, secs / 60 % 60, secs % 60);
        let place = if self.variant == 0 && self.home {
            "Home".to_string()
        } else {
            ENTRIES[self.cur].name.to_string()
        };
        let lines = vec![
            Line::from(format!("{rig}   {freq} MHz   {mode}   step 100   ● connected")),
            Line::from(format!("VFO A   split off   RX   S5   {clock}   Contacts 3")),
        ];
        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" rig-tui · {place} "));
        f.render_widget(Paragraph::new(lines).block(block), area);
    }

    fn item_line(&self, i: usize, cursor: Option<usize>, current: Option<usize>) -> Line<'static> {
        let e = item_entry(i);
        let ok = self.enabled(e);
        let mut text = format!(
            " {} {}",
            if current == Some(i) { "▶" } else { " " },
            item_label(i)
        );
        if !ok {
            text.push_str("  (needs FT-847)");
        }
        let mut style = if ok { Style::default() } else { Style::default().fg(Color::DarkGray) };
        if cursor == Some(i) {
            style = style.add_modifier(Modifier::REVERSED);
        }
        Line::from(Span::styled(text, style))
    }

    fn render_home(&self, f: &mut Frame, area: Rect) {
        let mut lines = vec![Line::from(Span::styled(
            "Activities",
            Style::default().add_modifier(Modifier::BOLD),
        ))];
        for i in 0..ENTRIES.len() {
            lines.push(self.item_line(i, Some(self.sel), None));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Upcoming plans",
            Style::default().add_modifier(Modifier::BOLD),
        )));
        for i in ENTRIES.len()..ITEMS {
            lines.push(self.item_line(i, Some(self.sel), None));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Recent contacts",
            Style::default().add_modifier(Modifier::BOLD),
        )));
        lines.extend(contact_lines());
        let block = Block::default().borders(Borders::ALL).title(" Home ");
        f.render_widget(Paragraph::new(lines).block(block), area);
    }

    fn render_rail(&self, f: &mut Frame, area: Rect) {
        let mut lines = vec![];
        for i in 0..ITEMS {
            if i == ENTRIES.len() {
                lines.push(Line::from(Span::styled(
                    " Plans",
                    Style::default().add_modifier(Modifier::BOLD),
                )));
            }
            lines.push(self.item_line(i, Some(self.sel), Some(self.sel)));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(" Alt-1..8  Alt-←/→"));
        let block = Block::default().borders(Borders::ALL).title(" Activities ");
        f.render_widget(Paragraph::new(lines).block(block), area);
    }

    fn render_tabs(&self, f: &mut Frame, area: Rect) {
        let mut spans = vec![];
        for (i, e) in ENTRIES.iter().enumerate() {
            let mut style = if self.enabled(i) { Style::default() } else { Style::default().fg(Color::DarkGray) };
            if i == self.cur {
                style = style.add_modifier(Modifier::REVERSED);
            }
            spans.push(Span::styled(format!(" {} ", e.short), style));
        }
        spans.push(Span::raw(" +2 plans (Alt-7/8)"));
        f.render_widget(Paragraph::new(Line::from(spans)), area);
    }

    fn render_activity(&self, f: &mut Frame, area: Rect) {
        let name = ENTRIES[self.cur].name;
        let lines: Vec<Line> = match self.cur {
            0 => {
                let mut l = vec![Line::from("Session: recent Contacts (no Mode panel for this mode)")];
                l.extend(contact_lines());
                l
            }
            1 => vec![
                Line::from("Tracking: AO-00 (made up)   AOS in 03:12   max el 54°"),
                Line::from(""),
                Line::from("  Pass table"),
                Line::from("   > 14:10  54°  ISS-fake   FM voice"),
                Line::from("     14:48  21°  XO-fake    linear"),
                Line::from("     16:21  12°  SO-fake    FM voice (short pass)"),
            ],
            2 | 4 => {
                let (a, b) = if self.cur == 2 {
                    ("SOTA W0X/XX-001", "Stations 3/4 (need 4 to qualify)")
                } else {
                    ("POTA K-0000", "QSOs 7/10 today (UTC)")
                };
                vec![
                    Line::from(format!("Activation: {a}   {b}")),
                    Line::from(""),
                    Line::from("  CW  > CQ CQ DE N0CALL/P SOTA K"),
                    Line::from("      < N0CALL/P DE T3ST 5NN"),
                    Line::from(""),
                    Line::from("  F1 CQ  F2 DE  F3 599 TU  F4 73  F5 AGN?  F6 QRZ?   (templated)"),
                ]
            }
            _ => {
                let prog = if self.cur == 3 { "SOTA" } else { "POTA" };
                vec![
                    Line::from(format!("{prog} spots (cached 41 s ago, next refresh in 19 s)")),
                    Line::from(""),
                    Line::from("   > 14.062  CW   W0X/XX-014   N0ABC    2 min"),
                    Line::from("     7.032   CW   K-0001       N0DEF    5 min"),
                    Line::from("     14.285  SSB  W0X/XX-022   N0GHI    9 min"),
                    Line::from(""),
                    Line::from("  Enter tunes the rig to the spot (a tune, never a transmit)"),
                ]
            }
        };
        let block = Block::default().borders(Borders::ALL).title(format!(" {name} "));
        f.render_widget(Paragraph::new(lines).block(block), area);
    }

    fn render_hint(&self, f: &mut Frame, area: Rect) {
        let nav = match self.variant {
            0 if self.home => "↑↓ Enter open  q quit",
            0 => "Esc Home",
            1 => "Alt-1..8 switch",
            _ => "^G go to",
        };
        let text = format!(
            "{}  {nav}  ^O cfg ^L log ^B bands ^R rig  {}",
            VARIANTS[self.variant], self.status
        );
        f.render_widget(
            Paragraph::new(text).style(Style::default().add_modifier(Modifier::REVERSED)),
            area,
        );
    }

    fn render_palette(&self, f: &mut Frame, area: Rect, filter: &str) {
        let w = area.width.min(48);
        let h = (self.filtered().len() as u16 + 3).min(area.height).min(12);
        let r = centered(area, w, h);
        let mut lines = vec![Line::from(format!("go to: {filter}▏"))];
        for (n, &i) in self.filtered().iter().enumerate() {
            lines.push(self.item_line(i, (n == self.pal_sel).then_some(i), None));
        }
        f.render_widget(Clear, r);
        f.render_widget(
            Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(" Go to ")),
            r,
        );
    }

    fn render_modal(&self, f: &mut Frame, area: Rect) {
        let (title, lines): (&str, Vec<Line>) = match self.modal {
            Modal::Config => (
                " Config ",
                vec![
                    Line::from("Callsign  N0CALL"),
                    Line::from("Locator   EN71 (placeholder)"),
                    Line::from("Rig       127.0.0.1:4532"),
                    Line::from("Soft keys F1..F6"),
                ],
            ),
            Modal::Log => (
                " Log Contact ",
                vec![
                    Line::from("Call     ________"),
                    Line::from("RST sent 599   RST rcvd 599"),
                    Line::from("(fields prefilled by the Activity)"),
                ],
            ),
            Modal::Band => (
                " Bandplan · 20 m ",
                vec![
                    Line::from("14.000 ──CW──┬─digital─┬────── phone ──────▶ 14.350"),
                    Line::from("                ▲ 14.060 (VFO)"),
                    Line::from("Enter on a segment tunes to its start (tune only)"),
                ],
            ),
            _ => (
                " Rig ",
                vec![
                    Line::from(format!("{} KX3     :4532", if self.rig == Rig::Kx3 { "●" } else { "○" })),
                    Line::from(format!("{} FT-847  :4533", if self.rig == Rig::Ft847 { "●" } else { "○" })),
                    Line::from("Enter/←/→ switches the Active rig"),
                ],
            ),
        };
        let r = centered(area, area.width.min(60), (lines.len() as u16 + 2).min(area.height));
        f.render_widget(Clear, r);
        f.render_widget(
            Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(title)),
            r,
        );
    }
}

fn contact_lines() -> Vec<Line<'static>> {
    vec![
        Line::from("  14:02Z  N0ABC   14.062 CW  599/599"),
        Line::from("  13:58Z  N0DEF    7.032 CW  579/599"),
        Line::from("  13:41Z  N0GHI   14.285 SSB 59/59"),
    ]
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    Rect {
        x: area.x + area.width.saturating_sub(w) / 2,
        y: area.y + area.height.saturating_sub(h) / 2,
        width: w.min(area.width),
        height: h.min(area.height),
    }
}

fn dump(w: u16, h: u16) {
    let cases: [(&str, usize, Option<usize>, bool); 5] = [
        ("A Home", 0, None, false),
        ("A in POTA Hunter", 0, Some(5), false),
        ("B in POTA Hunter", 1, Some(5), false),
        ("C in SOTA Activator", 2, Some(2), false),
        ("C palette open", 2, Some(2), true),
    ];
    for (label, variant, item, palette) in cases {
        let mut p = Proto::new(variant);
        if let Some(i) = item {
            p.launch(i);
        }
        if palette {
            p.palette = Some(String::new());
        }
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| p.render(f)).unwrap();
        println!("=== {label} at {w}x{h} ===");
        let buf = term.backend().buffer();
        for y in 0..h {
            let mut s = String::new();
            for x in 0..w {
                s.push_str(buf[(x, y)].symbol());
            }
            println!("{}", s.trim_end());
        }
    }
}

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if let Some(pos) = args.iter().position(|a| a == "--dump") {
        let size = args.get(pos + 1).map(String::as_str).unwrap_or("80x24");
        let (w, h) = size.split_once('x').unwrap_or(("80", "24"));
        dump(w.parse().unwrap_or(80), h.parse().unwrap_or(24));
        return Ok(());
    }
    let mut term = ratatui::init();
    let mut p = Proto::new(0);
    while !p.quit {
        term.draw(|f| p.render(f))?;
        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(k) = event::read()? {
                if k.kind == KeyEventKind::Press {
                    let before = p.variant;
                    p.on_key(k);
                    if p.variant != before {
                        let (cur, rig) = (p.cur, p.rig);
                        let mut n = Proto::new(p.variant);
                        n.cur = cur;
                        n.rig = rig;
                        n.sel = p.sel;
                        n.home = p.variant == 0 && cur == 0;
                        n.start = p.start;
                        p = n;
                    }
                }
            }
        }
    }
    ratatui::restore();
    Ok(())
}
