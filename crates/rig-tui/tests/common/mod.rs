//! A scripted stand-in for `rigctld`. The dummy rig can't emulate `TB;`, so this replays the
//! protocol framing recorded from `rigctld -m 1` (extended `+` mode, including the stray `\r\0`
//! after raw `w` replies) with canned answers. The `f`, `F`, `m` and `M` replies follow the
//! documented extended format and are scripted, not captured from a radio.
#![allow(dead_code)]

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use rig_tui::{App, Command, Rig, RigEvent, RigMode, render};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

pub struct FakeRigctld {
    pub addr: SocketAddr,
    /// Every command line received, e.g. `+w TB;` or `+b CQ`.
    pub received: Arc<Mutex<Vec<String>>>,
    /// Set to make the fake drop its connection, unanswered, on the next command it receives.
    pub hang_up: Arc<AtomicBool>,
}

/// What the fake rig reports for `f` and `m`; `F` and `M` change it.
#[derive(Clone)]
pub struct RigState {
    pub freq: u64,
    pub mode: String,
    pub passband: i64,
    pub vfo: String,
    pub split: bool,
    pub tx_vfo: String,
    /// What `i` reports. Scripted: whether a real KX3 reports the transmit VFO's frequency here
    /// is the open question, untested on hardware.
    pub tx_freq: u64,
    /// dB relative to S9, as `l STRENGTH` reports it.
    pub strength: i32,
    pub ptt: bool,
    /// Answer `\get_vfo` and `\set_vfo` with "function not available" (-11).
    pub no_vfo: bool,
    /// Answer `s`, `S`, `i` and `I` with "function not available" (-11).
    pub no_split: bool,
    /// Refuse `F` with a generic error (-1).
    pub refuse_set_freq: bool,
}

impl Default for RigState {
    fn default() -> Self {
        Self {
            freq: 14_074_000,
            mode: "CW".to_string(),
            passband: 500,
            vfo: "VFOA".to_string(),
            split: false,
            tx_vfo: "VFOA".to_string(),
            tx_freq: 0,
            strength: -54,
            ptt: false,
            no_vfo: false,
            no_split: false,
            refuse_set_freq: false,
        }
    }
}

impl FakeRigctld {
    /// `tb_replies` are returned, in order, for successive `TB;` polls; after that `TB000;`.
    pub async fn start(tb_replies: &[&str]) -> Self {
        Self::start_with(tb_replies, RigState::default()).await
    }

    pub async fn start_with(tb_replies: &[&str], state: RigState) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        Self::serve(listener, tb_replies, state)
    }

    /// Serve on an address that was reserved earlier (see [`reserve_addr`]).
    pub async fn start_at(addr: SocketAddr, tb_replies: &[&str], state: RigState) -> Self {
        let listener = TcpListener::bind(addr).await.unwrap();
        Self::serve(listener, tb_replies, state)
    }

    fn serve(listener: TcpListener, tb_replies: &[&str], mut state: RigState) -> Self {
        let addr = listener.local_addr().unwrap();
        let received = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&received);
        let hang_up = Arc::new(AtomicBool::new(false));
        let hang = Arc::clone(&hang_up);
        let mut replies: Vec<String> = tb_replies.iter().rev().map(|s| s.to_string()).collect();
        tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let (read, mut write) = stream.into_split();
                let mut lines = BufReader::new(read).lines();
                while let Some(line) = lines.next_line().await.unwrap() {
                    log.lock().unwrap().push(line.clone());
                    if hang.swap(false, Ordering::SeqCst) {
                        break;
                    }
                    let response = if line == "+w TB;" {
                        let reply = replies.pop().unwrap_or_else(|| "TB000;".to_string());
                        format!("send_cmd: TB;\nReply: {reply}\r\0RPRT 0\n")
                    } else if let Some(text) = line.strip_prefix("+b ") {
                        format!("send_morse: {text}\nRPRT 0\n")
                    } else if line == "+f" {
                        format!("get_freq:\nFrequency: {}\nRPRT 0\n", state.freq)
                    } else if line.starts_with("+F ") && state.refuse_set_freq {
                        "RPRT -1\n".to_string()
                    } else if let Some(hz) = line.strip_prefix("+F ") {
                        state.freq = hz.parse().unwrap();
                        format!("set_freq: {hz}\nRPRT 0\n")
                    } else if line.starts_with("+\\get_vfo") || line.starts_with("+\\set_vfo") {
                        if state.no_vfo {
                            "RPRT -11\n".to_string()
                        } else if let Some(vfo) = line.strip_prefix("+\\set_vfo ") {
                            state.vfo = vfo.to_string();
                            format!("set_vfo: {vfo}\nRPRT 0\n")
                        } else {
                            format!("get_vfo:\nVFO: {}\nRPRT 0\n", state.vfo)
                        }
                    } else if matches!(line.as_str(), "+s" | "+i")
                        || line.starts_with("+S ")
                        || line.starts_with("+I ")
                    {
                        if state.no_split {
                            "RPRT -11\n".to_string()
                        } else if line == "+s" {
                            format!(
                                "get_split_vfo:\nSplit: {}\nTX VFO: {}\nRPRT 0\n",
                                u8::from(state.split),
                                state.tx_vfo
                            )
                        } else if line == "+i" {
                            format!("get_split_freq:\nTX Frequency: {}\nRPRT 0\n", state.tx_freq)
                        } else if let Some(args) = line.strip_prefix("+S ") {
                            let (on, vfo) = args.split_once(' ').unwrap();
                            state.split = on == "1";
                            state.tx_vfo = vfo.to_string();
                            format!("set_split_vfo: {args}\nRPRT 0\n")
                        } else {
                            let hz = line.strip_prefix("+I ").unwrap();
                            state.tx_freq = hz.parse().unwrap();
                            format!("set_split_freq: {hz}\nRPRT 0\n")
                        }
                    } else if line == "+t" {
                        format!("get_ptt:\nPTT: {}\nRPRT 0\n", u8::from(state.ptt))
                    } else if line == "+l STRENGTH" {
                        format!("get_level: STRENGTH\n{}\nRPRT 0\n", state.strength)
                    } else if line == "+m" {
                        format!(
                            "get_mode:\nMode: {}\nPassband: {}\nRPRT 0\n",
                            state.mode, state.passband
                        )
                    } else if let Some(args) = line.strip_prefix("+M ") {
                        let (mode, passband) = args.split_once(' ').unwrap();
                        state.mode = mode.to_string();
                        state.passband = passband.parse().unwrap();
                        format!("set_mode: {args}\nRPRT 0\n")
                    } else {
                        "RPRT -11\n".to_string()
                    };
                    write.write_all(response.as_bytes()).await.unwrap();
                }
            }
        });
        Self {
            addr,
            received,
            hang_up,
        }
    }

    pub fn commands(&self) -> Vec<String> {
        self.received.lock().unwrap().clone()
    }
}

/// An address nothing is listening on, that a fake rig can later bind to.
pub async fn reserve_addr() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    listener.local_addr().unwrap()
}

pub fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

pub fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

pub fn type_str(app: &mut App, text: &str) {
    for c in text.chars() {
        assert_eq!(app.on_key(key(KeyCode::Char(c))), vec![]);
    }
}

pub fn mode_event(name: &str) -> RigEvent {
    RigEvent::Mode {
        mode: RigMode::parse(name),
        passband: 500,
    }
}

/// An app whose rig is connected and in `mode`.
pub fn connected_app_in(app: App, mode: &str) -> App {
    let mut app = app;
    app.on_rig(RigEvent::Connected);
    app.on_rig(RigEvent::Freq(14_074_000));
    app.on_rig(mode_event(mode));
    app
}

/// An app in CW with the input line focused.
pub fn cw_app() -> App {
    focus_input(connected_app_in(App::new(), "CW"))
}

pub fn focus_input(mut app: App) -> App {
    app.on_key(key(KeyCode::Tab));
    app
}

/// Feed the app one decoded-text reply, as the rig would after a `TB;` poll.
pub fn rx(app: &mut App, tb_reply: &str) {
    app.on_rig(RigEvent::PanelReply {
        request: "TB;".to_string(),
        reply: Ok(tb_reply.to_string()),
    });
}

pub fn screen(app: &App, width: u16, height: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| render(app, frame)).unwrap();
    let buffer = terminal.backend().buffer();
    buffer
        .content()
        .chunks(width as usize)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect())
        .collect()
}

/// Run `commands` against `rig` and feed the resulting events to `app`, like `main` does.
pub async fn dispatch(rig: &mut Rig, app: &mut App, commands: Vec<Command>) {
    for command in commands {
        for event in rig.execute(command).await {
            app.on_rig(event);
        }
    }
}

pub async fn tick(rig: &mut Rig, app: &mut App, now: Instant) {
    let commands = app.tick(now);
    dispatch(rig, app, commands).await;
}
