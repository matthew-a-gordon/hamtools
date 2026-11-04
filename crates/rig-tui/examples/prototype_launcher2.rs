//! PROTOTYPE 2 (throwaway; wayfinder ticket "Prototype: launcher home view and navigation").
//!
//! The navigation model the operator picked (a palette over an always-on Activity, chords only)
//! wrapped around the REAL screens: General is the real rig-tui, Satellite is the real sat-tui fed
//! from the cached TLEs and transmitters in ~/.cache/hamtools. The SOTA/POTA bodies are stubs on
//! top of the real rig-tui. The rig is fake; nothing is sent to rigctld, the Log or the network.
//! Do not save in the config dialog.
//!
//!   cargo run -p rig-tui --example prototype_launcher2
//!   cargo run -p rig-tui --example prototype_launcher2 -- --dump 100x30
//!
//! Chords (the only keys this layer owns; every other key goes to the real screen):
//!   Ctrl-G go to (palette)   Ctrl-K keybindings   Ctrl-B bandplan   Ctrl-R rig switcher   Ctrl-C quit
//! Plain `q` is ignored here on purpose.

use std::time::{Duration as StdDuration, Instant, SystemTime};

use chrono::{Duration, Utc};
use orbit::{Observer, parse_tles};
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::{Frame, Terminal};
use rig_tui::{App as RigApp, RigEvent, RigMode};
use sat_tui::app::{App as SatApp, Settings};
use sat_tui::satnogs::parse_transmitters;
use sat_tui::ui::{Footer, format_countdown, render as sat_render};

const DIM: Color = Color::DarkGray;

const ENTRIES: [&str; 6] = [
    "General",
    "Satellite",
    "SOTA Activator",
    "SOTA Chaser",
    "POTA Activator",
    "POTA Hunter",
];

// Made-up plans and references.
const PLANS: [(&str, usize); 2] = [("Plan  Sat 09:30  SOTA W0X/XX-001", 2), ("Plan  Sun 14:00  POTA K-0000", 4)];

#[derive(Clone, Copy, PartialEq)]
enum Rig {
    Kx3,
    Ft847,
}

#[derive(Clone, Copy, PartialEq)]
enum Modal {
    None,
    Band,
    Rig,
    Keys,
}

struct Row {
    label: String,
    state: String,
    entry: usize,
    ok: bool,
}

struct Proto {
    cur: usize,
    rig: Rig,
    modal: Modal,
    palette: Option<String>,
    pal_sel: usize,
    sat: SatApp,
    rigapp: RigApp,
    last_feed: Instant,
    status: String,
    quit: bool,
}

fn build_sat() -> Result<SatApp, String> {
    let dir = std::path::PathBuf::from(std::env::var("HOME").map_err(|e| e.to_string())?)
        .join(".cache/hamtools");
    let tles = std::fs::read_to_string(dir.join("tles.txt")).map_err(|e| format!("tles.txt: {e}"))?;
    let tx = std::fs::read_to_string(dir.join("transmitters.json"))
        .map_err(|e| format!("transmitters.json: {e}"))?;
    let sats = parse_tles(&tles).map_err(|e| e.to_string())?;
    let txs = parse_transmitters(&tx).map_err(|e| e.to_string())?;
    // Town-level Middlebury, IN (the repo's allowed default).
    let observer = Observer { lat_deg: 41.67, lon_deg: -85.71, alt_m: 250.0 };
    let settings = Settings { watch: vec![], horizon_hours: 24, min_elevation_deg: 10.0, short_pass_secs: 180 };
    let mut app = SatApp::new(sats, txs, observer, settings, Utc::now());
    app.set_rig_live(true);
    Ok(app)
}

fn feed(app: &mut RigApp, rig: Rig) {
    app.on_rig(RigEvent::Connected);
    let (hz, mode) = match rig {
        Rig::Kx3 => (14_060_000, RigMode::Cw),
        Rig::Ft847 => (145_900_000, RigMode::Fm),
    };
    app.on_rig(RigEvent::Freq(hz));
    app.on_rig(RigEvent::Mode { mode, passband: 400 });
    app.on_rig(RigEvent::Vfo("VFOA".into()));
    app.on_rig(RigEvent::Split { enabled: false, tx_vfo: "VFOA".into() });
    app.on_rig(RigEvent::Ptt(false));
    app.on_rig(RigEvent::Strength(-24));
}

impl Proto {
    fn new() -> Result<Self, String> {
        let mut rigapp = RigApp::new();
        rigapp.tick(Instant::now());
        rigapp.set_utc(SystemTime::now());
        feed(&mut rigapp, Rig::Kx3);
        Ok(Proto {
            cur: 0,
            rig: Rig::Kx3,
            modal: Modal::None,
            palette: None,
            pal_sel: 0,
            sat: build_sat()?,
            rigapp,
            last_feed: Instant::now(),
            status: String::new(),
            quit: false,
        })
    }

    fn enabled(&self, entry: usize) -> bool {
        !(entry == 1 && self.rig == Rig::Kx3)
    }

    fn next_pass(&self) -> String {
        match self.sat.rows().first() {
            Some(r) => format!("{} in {}", r.name, format_countdown(r.pass.aos - Utc::now())),
            None => "no passes".into(),
        }
    }

    fn rows(&self) -> Vec<Row> {
        let mut rows = vec![];
        for (label, entry) in PLANS {
            rows.push(Row { label: label.into(), state: "planned".into(), entry, ok: true });
        }
        for (i, name) in ENTRIES.iter().enumerate() {
            let ok = self.enabled(i);
            let state = match i {
                0 => "rig as it is".to_string(),
                1 if !ok => "needs FT-847".to_string(),
                1 => format!("next: {}", self.next_pass()),
                2 | 3 => "SOTA".to_string(),
                _ => "POTA".to_string(),
            };
            rows.push(Row { label: (*name).into(), state, entry: i, ok });
        }
        rows
    }

    fn filtered(&self) -> Vec<Row> {
        let f = self.palette.clone().unwrap_or_default().to_lowercase();
        self.rows().into_iter().filter(|r| r.label.to_lowercase().contains(&f)).collect()
    }

    fn go(&mut self, entry: usize) {
        if !self.enabled(entry) {
            self.status = "Satellite needs the FT-847 (sat mode)".into();
            return;
        }
        self.cur = entry;
        self.palette = None;
        self.status.clear();
    }

    fn on_key(&mut self, k: KeyEvent) {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl {
            match k.code {
                KeyCode::Char('c') => {
                    self.quit = true;
                    return;
                }
                KeyCode::Char('g') => {
                    self.modal = Modal::None;
                    self.palette = Some(String::new());
                    self.pal_sel = 0;
                    return;
                }
                KeyCode::Char('b') | KeyCode::Char('r') | KeyCode::Char('k') => {
                    let m = match k.code {
                        KeyCode::Char('b') => Modal::Band,
                        KeyCode::Char('r') => Modal::Rig,
                        _ => Modal::Keys,
                    };
                    self.palette = None;
                    self.modal = if self.modal == m { Modal::None } else { m };
                    return;
                }
                _ => {}
            }
        }
        if self.modal != Modal::None {
            match k.code {
                KeyCode::Esc => self.modal = Modal::None,
                KeyCode::Enter | KeyCode::Left | KeyCode::Right if self.modal == Modal::Rig => {
                    self.rig = if self.rig == Rig::Kx3 { Rig::Ft847 } else { Rig::Kx3 };
                    feed(&mut self.rigapp, self.rig);
                    if !self.enabled(self.cur) {
                        self.cur = 0;
                        self.status = "Rig switched; Satellite stops, back to General".into();
                    }
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
                    if let Some(r) = self.filtered().get(self.pal_sel) {
                        let e = r.entry;
                        self.go(e);
                    }
                }
                _ => {}
            }
            return;
        }
        // Everything else belongs to the real screen. Commands they return are dropped (no I/O).
        if self.cur == 1 {
            let _ = self.sat.handle_key(k);
        } else {
            let _ = self.rigapp.on_key(k);
        }
    }

    fn tick(&mut self) {
        let _ = self.sat.tick(Utc::now());
        self.rigapp.tick(Instant::now());
        self.rigapp.set_utc(SystemTime::now());
        if self.last_feed.elapsed() > StdDuration::from_millis(500) {
            feed(&mut self.rigapp, self.rig);
            self.last_feed = Instant::now();
        }
    }

    // ---- rendering ----

    fn render(&self, f: &mut Frame) {
        let area = f.area();
        let [bar, body] = Layout::vertical([Constraint::Length(1), Constraint::Min(3)]).areas(area);
        self.render_bar(f, bar);
        match self.cur {
            1 => blit(f, body, |i| sat_render(&self.sat, &self.footer(), i)),
            0 => blit(f, body, |i| rig_tui::render(&self.rigapp, i)),
            2 | 4 => {
                let [banner, rest] = Layout::vertical([Constraint::Length(1), Constraint::Min(3)]).areas(body);
                let (a, b) = if self.cur == 2 {
                    ("SOTA W0X/XX-001", "stations 3/4 (4 to qualify)")
                } else {
                    ("POTA K-0000", "QSOs 7/10 today (UTC)")
                };
                f.render_widget(
                    Paragraph::new(Line::from(vec![
                        Span::styled(format!(" {a} "), Style::default().fg(Color::Black).bg(Color::Cyan)),
                        Span::raw(" "),
                        Span::styled(b, Style::default().fg(Color::Yellow)),
                        Span::styled("   (activation tracker: stub)", Style::default().fg(DIM)),
                    ])),
                    banner,
                );
                blit(f, rest, |i| rig_tui::render(&self.rigapp, i));
            }
            _ => {
                let h = (body.height / 3).clamp(5, 9);
                let [spots, rest] = Layout::vertical([Constraint::Length(h), Constraint::Min(3)]).areas(body);
                self.render_spots(f, spots);
                blit(f, rest, |i| rig_tui::render(&self.rigapp, i));
            }
        }
        if let Some(filter) = &self.palette {
            self.render_palette(f, area, filter);
        }
        match self.modal {
            Modal::Band => self.render_band(f, area),
            Modal::Rig => self.render_rig(f, area),
            Modal::Keys => self.render_keys(f, area),
            Modal::None => {}
        }
    }

    fn footer(&self) -> Footer {
        Footer {
            now: Utc::now(),
            tle_age: Some(Duration::hours(3)),
            transmitter_age: Some(Duration::hours(3)),
            tles_stale: false,
            rig: "FT-847 :4533 connected".into(),
        }
    }

    fn render_bar(&self, f: &mut Frame, area: Rect) {
        let rig = match self.rig {
            Rig::Kx3 => "KX3 :4532",
            Rig::Ft847 => "FT-847 :4533",
        };
        let line = Line::from(vec![
            Span::styled(
                format!(" ▾ {} ", ENTRIES[self.cur]),
                Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  ^G go to  ^K keys  ^B bands  ^R rig  ", Style::default().fg(DIM)),
            Span::styled(format!("{rig} ●"), Style::default().fg(Color::Green)),
            Span::styled(format!("  {}", self.status), Style::default().fg(Color::Yellow)),
        ]);
        f.render_widget(Paragraph::new(line), area);
    }

    fn render_spots(&self, f: &mut Frame, area: Rect) {
        let prog = if self.cur == 3 { "SOTA" } else { "POTA" };
        let g = Style::default().fg(Color::Green);
        let y = Style::default().fg(Color::Yellow);
        let d = Style::default().fg(DIM);
        let lines = vec![
            Line::from(Span::styled(format!("{prog} spots · cached 41 s ago · next refresh 19 s · (stub)"), d)),
            Line::from(Span::styled(
                "> 14.062  CW   W0X/XX-014  N0ABC   2 min   fresh",
                g.add_modifier(Modifier::REVERSED),
            )),
            Line::from(Span::styled("  7.032   CW   K-0001      N0DEF   5 min   fresh", g)),
            Line::from(Span::styled("  14.285  SSB  W0X/XX-022  N0GHI   9 min   aging", y)),
            Line::from(Span::styled("  10.118  CW   W0X/XX-031  N0JKL  27 min   stale", d)),
            Line::from(Span::styled("Enter tunes the rig to the spot (a tune, never a transmit)", d)),
        ];
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(format!(" {} ", ENTRIES[self.cur]));
        f.render_widget(Paragraph::new(lines).block(block), area);
    }

    fn render_palette(&self, f: &mut Frame, area: Rect, filter: &str) {
        let rows = self.filtered();
        let w = area.width.min(62);
        let h = (rows.len() as u16 + 3).min(area.height).min(14);
        let r = centered(area, w, h);
        let mut lines = vec![Line::from(vec![Span::styled("go to: ", Style::default().fg(DIM)), Span::raw(format!("{filter}▏"))])];
        for (n, row) in rows.iter().enumerate() {
            let mut name = if row.ok { Style::default() } else { Style::default().fg(DIM) };
            let state = if row.ok { Style::default().fg(Color::Yellow) } else { Style::default().fg(DIM) };
            if n == self.pal_sel {
                name = name.fg(Color::Black).bg(Color::Cyan);
            }
            lines.push(Line::from(vec![
                Span::styled(format!(" {:<34}", row.label), name),
                Span::styled(format!(" {}", row.state), state),
            ]));
        }
        f.render_widget(Clear, r);
        f.render_widget(
            Paragraph::new(lines).block(
                Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan)).title(" Go to "),
            ),
            r,
        );
    }

    fn render_rig(&self, f: &mut Frame, area: Rect) {
        let dot = |on: bool| if on { Span::styled("● ", Style::default().fg(Color::Green)) } else { Span::raw("○ ") };
        let lines = vec![
            Line::from(vec![dot(self.rig == Rig::Kx3), Span::raw("KX3     :4532")]),
            Line::from(vec![dot(self.rig == Rig::Ft847), Span::raw("FT-847  :4533")]),
            Line::from(Span::styled("Enter/←/→ switches the Active rig · Esc closes", Style::default().fg(DIM))),
        ];
        let r = centered(area, area.width.min(56), 5.min(area.height));
        f.render_widget(Clear, r);
        f.render_widget(Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(" Rig ")), r);
    }

    fn render_keys(&self, f: &mut Frame, area: Rect) {
        let head = |t: &str| Line::from(Span::styled(t.to_string(), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)));
        let key = |k: &str, d: &str| {
            Line::from(vec![
                Span::styled(format!(" {k:<10}"), Style::default().fg(Color::Yellow)),
                Span::raw(d.to_string()),
            ])
        };
        let mut lines = vec![head(&format!("{} (this screen)", ENTRIES[self.cur]))];
        if self.cur == 1 {
            lines.extend([
                key("j k ↑ ↓", "move through passes"),
                key("t", "start or stop tracking"),
                key("s", "leave sat mode"),
                key("r", "refresh TLEs and transmitters"),
                key("l / L", "log a Contact / session list"),
            ]);
        } else {
            lines.extend([
                key("Tab", "move focus strip ↔ panel"),
                key("f ← →", "frequency / tune (strip focused)"),
                key("m M", "mode (strip focused)"),
                key("v s S", "VFO, split (strip focused)"),
                key("F1-F6", "soft keys (CW)"),
                key("l / L / c", "log form / session list / config"),
            ]);
        }
        lines.push(head("Everywhere"));
        lines.extend([
            key("Ctrl-G", "go to an Activity or plan"),
            key("Ctrl-B", "bandplan"),
            key("Ctrl-R", "switch rig"),
            key("Ctrl-K", "this list"),
            key("Ctrl-C", "quit"),
        ]);
        lines.push(Line::from(Span::styled(
            " mock: the real list is generated from the bindings table (#49)",
            Style::default().fg(DIM),
        )));
        let r = centered(area, area.width.min(60), (lines.len() as u16 + 2).min(area.height));
        f.render_widget(Clear, r);
        f.render_widget(
            Paragraph::new(lines).block(
                Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan)).title(" Keybindings "),
            ),
            r,
        );
    }

    fn render_band(&self, f: &mut Frame, area: Rect) {
        const W: usize = 70; // 5 kHz per cell across 14.000-14.350
        // (start kHz offset, end, label, bg, in privilege for a General)
        let segs: [(usize, usize, &str, Color, bool); 6] = [
            (0, 25, "CW", Color::Blue, false),
            (25, 70, "CW", Color::Blue, true),
            (70, 95, "DIG", Color::Magenta, true),
            (95, 150, "CW/DIG", Color::Blue, true),
            (150, 225, "PHONE", Color::Green, false),
            (225, 350, "PHONE", Color::Green, true),
        ];
        let mut bar: Vec<Span> = vec![];
        for (a, b, label, bg, ok) in segs {
            let n = (b - a) / 5;
            let text = format!("{:^n$}", if ok { label.to_string() } else { format!("{label}✗") });
            let style = if ok {
                Style::default().fg(Color::White).bg(bg)
            } else {
                Style::default().fg(DIM).bg(Color::Black).add_modifier(Modifier::CROSSED_OUT)
            };
            bar.push(Span::styled(text, style));
        }
        let mut spots = vec![' '; W];
        for khz in [62usize, 285] {
            spots[khz / 5] = '•';
        }
        let mut marker = vec![' '; W];
        marker[60 / 5] = '▲';
        let bands = ["160", "80", "60", "40", "30", "20", "17", "15", "12", "10", "6", "2", "70"];
        let tabs: Vec<Span> = bands
            .iter()
            .flat_map(|b| {
                let s = if *b == "20" { Style::default().add_modifier(Modifier::REVERSED) } else { Style::default() };
                [Span::styled(format!(" {b} "), s)]
            })
            .collect();
        let lines = vec![
            Line::from(tabs),
            Line::from(""),
            Line::from(Span::styled("14.000      .050      .100      .150      .200      .250      .300    14.350", Style::default().fg(DIM))),
            Line::from(Span::styled(spots.into_iter().collect::<String>(), Style::default().fg(Color::Yellow))),
            Line::from(bar),
            Line::from(Span::styled(marker.into_iter().collect::<String>(), Style::default().fg(Color::Cyan))),
            Line::from(vec![
                Span::styled("▲ ", Style::default().fg(Color::Cyan)),
                Span::raw("VFO 14.060   "),
                Span::styled("• ", Style::default().fg(Color::Yellow)),
                Span::raw("hunter spots   "),
                Span::styled("✗", Style::default().fg(DIM)),
                Span::raw(" outside General privileges"),
            ]),
            Line::from(Span::styled(
                "Illustrative only: segments and privileges are NOT verified (see the band-plan research).",
                Style::default().fg(DIM),
            )),
            Line::from(Span::styled("←/→ band · Enter on a segment tunes to its start · Esc", Style::default().fg(DIM))),
        ];
        let r = centered(area, area.width.min(74), (lines.len() as u16 + 2).min(area.height));
        f.render_widget(Clear, r);
        f.render_widget(
            Paragraph::new(lines).block(
                Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan)).title(" Bandplan · 20 m "),
            ),
            r,
        );
    }
}

/// Run a real screen's `render` on a private terminal of the body's size and copy its cells
/// (colors included) into the outer frame.
fn blit(f: &mut Frame, area: Rect, draw: impl FnOnce(&mut Frame)) {
    let mut term = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
    term.draw(|inner| draw(inner)).unwrap();
    let src = term.backend().buffer().clone();
    for y in 0..area.height {
        for x in 0..area.width {
            f.buffer_mut()[(area.x + x, area.y + y)] = src[(x, y)].clone();
        }
    }
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    Rect {
        x: area.x + area.width.saturating_sub(w) / 2,
        y: area.y + area.height.saturating_sub(h) / 2,
        width: w.min(area.width),
        height: h.min(area.height),
    }
}

fn dump(w: u16, h: u16) -> Result<(), String> {
    let cases: [(&str, usize, Option<&str>, Option<Modal>); 7] = [
        ("General (real rig-tui)", 0, None, None),
        ("Satellite after Ctrl-R to FT-847 (real sat-tui)", 1, None, None),
        ("POTA Activator", 4, None, None),
        ("POTA Hunter", 5, None, None),
        ("Palette", 1, Some(""), None),
        ("Bandplan", 3, None, Some(Modal::Band)),
        ("Keybindings", 1, None, Some(Modal::Keys)),
    ];
    for (label, cur, palette, modal) in cases {
        let mut p = Proto::new()?;
        if cur == 1 {
            p.rig = Rig::Ft847;
        }
        p.cur = cur;
        p.palette = palette.map(String::from);
        p.modal = modal.unwrap_or(Modal::None);
        p.tick();
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
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if let Some(pos) = args.iter().position(|a| a == "--dump") {
        let size = args.get(pos + 1).map(String::as_str).unwrap_or("100x30");
        let (w, h) = size.split_once('x').unwrap_or(("100", "30"));
        dump(w.parse().unwrap_or(100), h.parse().unwrap_or(30))?;
        return Ok(());
    }
    let mut p = Proto::new()?;
    let mut term = ratatui::init();
    while !p.quit {
        p.tick();
        term.draw(|f| p.render(f))?;
        if event::poll(StdDuration::from_millis(250))? {
            if let Event::Key(k) = event::read()? {
                if k.kind == KeyEventKind::Press {
                    p.on_key(k);
                }
            }
        }
    }
    ratatui::restore();
    Ok(())
}
