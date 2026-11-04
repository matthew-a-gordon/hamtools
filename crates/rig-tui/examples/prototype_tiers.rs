//! PROTOTYPE 3 (throwaway; wayfinder ticket "Size tiers above 80x24: breakpoints and extra panes").
//!
//! Question: which panes appear at which terminal size, for the Hunter and Activator screens?
//! Model (decided with the operator): width and height are SEPARATE axes.
//!   width  >= 128: second column; >= 240: third column
//!   height >= 30: bandplan strip (+ nearby spots on the Activator); >= 40: key hints block, and
//!          today's contacts stacked in the main column when there is no second column
//! Below 80x24 only a "window too small" notice is shown. The operator's real sizes: half screen
//! 83x47, fullscreen 172x48.
//!
//! The top strip and Mode panel are faked here (the real ones were shown in prototype_launcher2);
//! the pane CONTENTS are the ones decided in the "Hunter and Activator screens" ticket, filled with
//! made-up data. Nothing touches a rig, the Log or the network.
//!
//!   cargo run -p rig-tui --example prototype_tiers                 (live: resize the terminal)
//!   cargo run -p rig-tui --example prototype_tiers -- --dump 83x47
//!   cargo run -p rig-tui --example prototype_tiers -- --all        (operator sizes + candidates)
//!
//! Live keys: Tab switch Hunter/Activator, q quit.

use std::time::Duration;

use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::{Frame, Terminal};

const DIM: Color = Color::DarkGray;

#[derive(Clone, Copy, PartialEq)]
enum Screen {
    Hunter,
    Activator,
}

/// What fits at this size. The one shared function the spec calls for.
#[derive(Clone, Copy)]
struct Fit {
    cols: u8,       // 1, 2 or 3 columns
    band: bool,     // bandplan strip
    hints: bool,    // key hints block
    stacked: bool,  // today's contacts stacked in the main column (no second column)
}

fn fit(w: u16, h: u16) -> Option<Fit> {
    if w < 80 || h < 24 {
        return None;
    }
    let cols = if w >= 240 { 3 } else if w >= 128 { 2 } else { 1 };
    Some(Fit { cols, band: h >= 30, hints: h >= 40, stacked: cols == 1 && h >= 40 })
}

fn pane(f: &mut Frame, area: Rect, title: &str, lines: Vec<Line>) {
    let block = Block::default().borders(Borders::ALL).title(format!(" {title} "));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let n = inner.height as usize;
    f.render_widget(Paragraph::new(lines.into_iter().take(n).collect::<Vec<_>>()), inner);
}

fn plain(s: &str) -> Line<'static> {
    Line::from(s.to_string())
}

fn dim(s: &str) -> Line<'static> {
    Line::from(Span::styled(s.to_string(), Style::default().fg(DIM)))
}

fn strip(f: &mut Frame, area: Rect, label: &str) {
    let cy = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);
    f.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled(format!(" {label} "), Style::default().fg(Color::Black).bg(Color::Cyan)),
                Span::raw("  KX3 "),
                Span::styled("connected", Style::default().fg(Color::Green)),
                Span::raw("   "),
                Span::styled("14.062.000", cy),
                Span::raw("  CW 400   VFO A  split off   S5   Ctrl-G go to  Ctrl-K keys   14:31:07Z"),
            ]),
            dim(&"─".repeat(area.width as usize)),
        ]),
        area,
    );
}

// Spot rows: age, call, MHz, mode, ref, km, comment.
const SPOTS: [(&str, &str, &str, &str, &str, &str, &str); 14] = [
    ("45s", "K9ABC", "14.062.0", "CW", "US-1234", "180", "CQ POTA"),
    ("1m", "W8XYZ", "7.030.5", "CW", "US-0042", "95", "tnx qso"),
    ("2m", "N4QQ", "14.285.0", "SSB", "US-3315", "790", "loud in IN"),
    ("3m", "KD2MM", "21.044.0", "CW", "US-8801", "930", ""),
    ("4m", "W1AW", "14.074.0", "FT8", "US-0008", "1100", "ft8"),
    ("5m", "AA0Z", "7.188.0", "SSB", "US-2210", "540", "QRV"),
    ("6m", "K0ZZ", "10.118.0", "CW", "US-5001", "820", ""),
    ("8m", "VE3QQ", "14.250.0", "SSB", "CA-0150", "610", "P2P hunters"),
    ("9m", "N8RR", "7.035.0", "CW", "US-1190", "60", ""),
    ("11m", "W9TT", "18.130.0", "SSB", "US-4412", "300", ""),
    ("12m", "K5PP", "14.060.0", "CW", "US-0777", "1300", "QRP"),
    ("14m", "WB6II", "28.400.0", "SSB", "US-0031", "2900", ""),
    ("16m", "K3LL", "3.552.0", "CW", "US-0612", "700", ""),
    ("19m", "N1MM", "14.070.0", "Data", "US-0901", "1500", "psk"),
];

fn spot_lines(n: usize, wide: bool) -> Vec<Line<'static>> {
    let hdr = format!(
        "{:<4} {:<7} {:<9} {:<5} {:<8} {:>5}  {}",
        "age", "call", "MHz", "mode", "ref", "km", if wide { "comment" } else { "" }
    );
    let mut v = vec![Line::from(Span::styled(hdr, Style::default().add_modifier(Modifier::BOLD)))];
    for (i, s) in SPOTS.iter().enumerate().take(n.saturating_sub(1)) {
        let mark = match i {
            1 => "✓",
            0 => "P",
            _ => " ",
        };
        let style = if i == 1 || i > 5 { Style::default().fg(DIM) } else { Style::default() };
        let oop = if s.2.starts_with("14.285") { "!" } else { " " };
        let cm = if wide { s.6 } else { "" };
        let text = format!("{mark}{:<3} {:<7} {:<8}{oop} {:<5} {:<8} {:>5}  {cm}", s.0, s.1, s.2, s.3, s.4, s.5);
        let style = if i == 0 { style.add_modifier(Modifier::REVERSED) } else { style };
        v.push(Line::from(Span::styled(text, style)));
    }
    v
}

fn bandplan(f: &mut Frame, area: Rect) {
    pane(
        f,
        area,
        "Bandplan 20m",
        vec![
            plain("14.000 ▒▒▒CW▒▒▒ ░░data░░ ▓▓▓▓▓▓▓▓▓ phone ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓ 14.350"),
            plain("       ^14.062 RX   spots: · · ·· ·      · ·   ·"),
        ],
    );
}

fn hints(f: &mut Frame, area: Rect) {
    pane(
        f,
        area,
        "Keys",
        vec![
            plain("j/k select  Enter log  t tune  r refresh  d sort  b band  o mode  w hide worked  / search"),
            plain("f freq  m mode  v vfo  s split  S tx freq  </> tune   Ctrl-L log form  Ctrl-J session  Ctrl-K all keys"),
        ],
    );
}

fn today(n: usize) -> Vec<Line<'static>> {
    let rows = [
        "1425 W8AAA  CW  599 599",
        "1419 K9BBB  CW  579 599  P",
        "1411 N4CCC  CW  599 559",
        "1402 W1DDD  CW  599 599",
        "1355 K0EEE  CW  449 599",
        "1349 AA9FFF CW  599 599",
        "1340 W2GGG  CW  599 599",
        "1331 K8HHH  CW  589 599",
    ];
    rows.iter().take(n).map(|r| plain(r)).collect()
}

fn hunter(f: &mut Frame, area: Rect, x: Fit) {
    let [body, foot] = Layout::vertical([Constraint::Min(5), Constraint::Length(1)]).areas(area);
    f.render_widget(Paragraph::new(Span::styled(" 3 logged today   spots: 19 (next in 41s)   filters: none", Style::default().fg(DIM))), foot);
    let cols = match x.cols {
        1 => vec![Constraint::Percentage(100)],
        2 => vec![Constraint::Percentage(62), Constraint::Percentage(38)],
        _ => vec![Constraint::Percentage(46), Constraint::Percentage(28), Constraint::Percentage(26)],
    };
    let areas = Layout::horizontal(cols).split(body);
    let mut rows = vec![Constraint::Min(6), Constraint::Length(1)];
    if x.band {
        rows.push(Constraint::Length(4));
    }
    rows.push(Constraint::Length(if x.cols == 1 && !x.band { 3 } else { 4 }));
    if x.stacked {
        rows.push(Constraint::Length(8));
    }
    if x.hints {
        rows.push(Constraint::Length(4));
    }
    let main = Layout::vertical(rows).split(areas[0]);
    let mut i = 0;
    let list_h = main[0].height.saturating_sub(2) as usize;
    pane(f, main[i], "POTA Hunter spots", spot_lines(list_h, x.cols >= 2));
    i += 1;
    f.render_widget(
        Paragraph::new(Span::styled(" US-1234 Pinhook Bog · EN61 · 180 km SW", Style::default().fg(Color::Yellow))),
        main[i],
    );
    i += 1;
    if x.band {
        bandplan(f, main[i]);
        i += 1;
    }
    pane(f, main[i], "CW decode / send", vec![dim("CQ POTA K9ABC K9ABC"), plain("> ")]);
    i += 1;
    if x.stacked {
        pane(f, main[i], "Today (session)", today(main[i].height.saturating_sub(2) as usize));
        i += 1;
    }
    if x.hints {
        hints(f, main[i]);
    }
    if x.cols >= 2 {
        let side = Layout::vertical([Constraint::Length(9), Constraint::Min(4)]).split(areas[1]);
        pane(
            f,
            side[0],
            "Selected spot",
            vec![
                plain("K9ABC  US-1234"),
                plain("Pinhook Bog"),
                plain("LaPorte, IN  EN61"),
                plain("180 km SW  bearing 225°"),
                plain("14.062.0 CW  spotted 45s ago"),
                dim("comment: CQ POTA"),
                dim("park cache: 3 d old"),
            ],
        );
        pane(f, side[1], "Today (session)", today(side[1].height.saturating_sub(2) as usize));
    }
    if x.cols >= 3 {
        let c = Layout::vertical([Constraint::Percentage(45), Constraint::Min(4)]).split(areas[2]);
        pane(
            f,
            c[0],
            "Upcoming plans",
            vec![plain("Sat 09:30  SOTA W0X/XX-001"), plain("Sun 14:00  POTA US-1234"), dim("planned 2, done 5")],
        );
        pane(
            f,
            c[1],
            "Worked today by band",
            vec![plain("40m CW   1"), plain("20m CW   2"), dim("(derived from the Log)")],
        );
    }
}

fn activator(f: &mut Frame, area: Rect, x: Fit) {
    let cols = match x.cols {
        1 => vec![Constraint::Percentage(100)],
        2 => vec![Constraint::Percentage(56), Constraint::Percentage(44)],
        _ => vec![Constraint::Percentage(42), Constraint::Percentage(30), Constraint::Percentage(28)],
    };
    let areas = Layout::horizontal(cols).split(area);
    let mut rows = vec![
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Length(3),
        Constraint::Length(4),
        Constraint::Length(1),
    ];
    if x.band {
        rows.push(Constraint::Length(4));
    }
    if x.hints {
        rows.push(Constraint::Length(4));
    }
    rows.push(Constraint::Min(0));
    let m = Layout::vertical(rows).split(areas[0]);
    f.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled(" POTA US-1234 ", Style::default().fg(Color::Black).bg(Color::Cyan)),
                Span::raw(" Pinhook Bog, IN · EN61 · 180 km · 2026-11-05 · "),
                Span::styled("active", Style::default().fg(Color::Green)),
            ]),
            Line::from(Span::styled(" you are spotted: 14.062 CW by W1XYZ 3m ago", Style::default().fg(Color::Yellow))),
        ]),
        m[0],
    );
    f.render_widget(
        Paragraph::new(Span::styled(" POTA US-1234: 6 of 10 (UTC day)", Style::default().fg(Color::Yellow))),
        m[1],
    );
    pane(f, m[2], "Contact", vec![plain("Call: K9AB_     RST 599/599   Their ref: "), dim("Enter save · Tab fields · Esc rig keys")]);
    pane(f, m[3], "CW decode / send", vec![dim("K9AB DE K9XYZ 5NN"), plain("> ")]);
    f.render_widget(
        Paragraph::new(Span::styled(" F1 CQ  F2 EXCH  F3 TU  F4 AGN  F5 QRZ  F6 73", Style::default().fg(Color::Magenta))),
        m[4],
    );
    let mut i = 5;
    if x.band {
        bandplan(f, m[i]);
        i += 1;
    }
    if x.hints {
        hints(f, m[i]);
        i += 1;
    }
    if x.cols == 1 {
        pane(f, m[i], "Recent", today(m[i].height.saturating_sub(2) as usize));
        return;
    }
    // Second column: nearby spots only when there is room in height too.
    let r = if x.band {
        Layout::vertical([Constraint::Percentage(40), Constraint::Min(4)]).split(areas[1])
    } else {
        Layout::vertical([Constraint::Min(4)]).split(areas[1])
    };
    let mut ri = 0;
    if x.band {
        pane(f, r[0], "Nearby spots (P2P)", spot_lines(r[0].height.saturating_sub(2) as usize, true));
        ri = 1;
    }
    pane(f, r[ri], "Recent contacts", today(r[ri].height.saturating_sub(2) as usize));
    if x.cols >= 3 {
        let c = Layout::vertical([Constraint::Length(9), Constraint::Min(4)]).split(areas[2]);
        pane(
            f,
            c[0],
            "Reference",
            vec![plain("US-1234  Pinhook Bog"), plain("LaPorte, IN  EN61"), plain("180 km SW"), dim("cache 3 d old")],
        );
        pane(f, c[1], "Upcoming plans", vec![plain("Sat 09:30  SOTA W0X/XX-001"), plain("Sun 14:00  POTA US-1234")]);
    }
}

fn render(f: &mut Frame, screen: Screen) {
    let a = f.area();
    let Some(x) = fit(a.width, a.height) else {
        f.render_widget(
            Paragraph::new(format!("Window too small: needs 80x24, have {}x{}", a.width, a.height)),
            a,
        );
        return;
    };
    let [top, body] = Layout::vertical([Constraint::Length(2), Constraint::Min(5)]).areas(a);
    let label = match screen {
        Screen::Hunter => "POTA Hunter",
        Screen::Activator => "POTA Activator",
    };
    strip(f, top, &format!("{label}  {}x{} cols={} band={} hints={}", a.width, a.height, x.cols, x.band, x.hints));
    match screen {
        Screen::Hunter => hunter(f, body, x),
        Screen::Activator => activator(f, body, x),
    }
}

fn dump(w: u16, h: u16) {
    for (name, s) in [("Hunter", Screen::Hunter), ("Activator", Screen::Activator)] {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| render(f, s)).unwrap();
        println!("=== {name} at {w}x{h} ===");
        let buf = term.backend().buffer();
        for y in 0..h {
            let line: String = (0..w).map(|x| buf[(x, y)].symbol().to_string()).collect();
            println!("{}", line.trim_end());
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--all") {
        for (w, h) in [(70, 20), (80, 24), (83, 47), (128, 26), (172, 48), (240, 67)] {
            dump(w, h);
        }
        return Ok(());
    }
    if let Some(pos) = args.iter().position(|a| a == "--dump") {
        let size = args.get(pos + 1).map(String::as_str).unwrap_or("80x24");
        let (w, h) = size.split_once('x').unwrap_or(("80", "24"));
        dump(w.parse().unwrap_or(80), h.parse().unwrap_or(24));
        return Ok(());
    }
    let mut screen = Screen::Hunter;
    let mut term = ratatui::init();
    loop {
        term.draw(|f| render(f, screen))?;
        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(k) = event::read()? {
                if k.kind == KeyEventKind::Press {
                    match k.code {
                        KeyCode::Char('q') => break,
                        KeyCode::Tab => {
                            screen = if screen == Screen::Hunter { Screen::Activator } else { Screen::Hunter }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    ratatui::restore();
    Ok(())
}
