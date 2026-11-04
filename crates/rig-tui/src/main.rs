use std::error::Error;
use std::future::Future;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use rig_tui::{App, Command, Config, Session, config_path, render};
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use rigctld_client::Client;
use tokio::runtime::Runtime;

/// The KX3 clears its decode buffer on each `TB;` and holds only 40 characters.
const POLL_INTERVAL: Duration = Duration::from_millis(100);
/// A stalled rigctld must not freeze the screen.
const RIG_TIMEOUT: Duration = Duration::from_secs(2);

const USAGE: &str = "usage: rig-tui [--config PATH] [ADDR]

ADDR is the KX3's rigctld; it overrides the config file (default 127.0.0.1:4532).
Config defaults to $XDG_CONFIG_HOME/hamtools/config.toml (see config.example.toml).";

fn main() -> ExitCode {
    match try_main() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("rig-tui: {e}");
            ExitCode::FAILURE
        }
    }
}

fn try_main() -> Result<(), Box<dyn Error>> {
    let mut config_arg: Option<PathBuf> = None;
    let mut addr_arg: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            "--config" => config_arg = Some(args.next().ok_or("--config needs a path")?.into()),
            _ if arg.starts_with('-') => {
                return Err(format!("unknown option {arg}\n{USAGE}").into());
            }
            _ => addr_arg = Some(arg),
        }
    }
    let default_path = config_path(
        std::env::var("XDG_CONFIG_HOME").ok().as_deref(),
        std::env::var("HOME").ok().as_deref(),
    );
    let config = Config::load(config_arg.as_deref(), default_path.as_deref())?;
    let addr = addr_arg.unwrap_or_else(|| config.kx3.addr.clone());

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let client = rt
        .block_on(Client::connect(&addr))
        .map_err(|e| format!("cannot reach rigctld at {addr}: {e}"))?;
    let mut session = Session::new(client);

    let mut terminal = ratatui::init();
    let result = run(&mut terminal, &rt, &mut session, config);
    ratatui::restore();
    result
}

fn with_timeout<T, E: Error + 'static>(
    rt: &Runtime,
    fut: impl Future<Output = Result<T, E>>,
) -> Result<T, String> {
    match rt.block_on(async { tokio::time::timeout(RIG_TIMEOUT, fut).await }) {
        Ok(result) => result.map_err(|e| e.to_string()),
        Err(_) => Err("rigctld timed out".to_string()),
    }
}

fn run(
    terminal: &mut DefaultTerminal,
    rt: &Runtime,
    session: &mut Session,
    config: Config,
) -> Result<(), Box<dyn Error>> {
    let mut app = App::with_callsign(config.callsign).with_macros(config.macros);
    let mut poll_failed = false;
    loop {
        terminal.draw(|frame| render(&app, frame))?;

        if event::poll(POLL_INTERVAL)?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            match app.handle_key(key) {
                Some(Command::Quit) => return Ok(()),
                Some(Command::Send(text)) => {
                    if let Err(e) = with_timeout(rt, session.send(&text)) {
                        app.set_status(Some(format!("send failed: {e}")));
                    }
                }
                None => {}
            }
        }

        match with_timeout(rt, session.poll()) {
            Ok(polled) => {
                app.on_rx(&polled.text);
                if poll_failed {
                    app.set_status(None);
                    poll_failed = false;
                }
            }
            Err(e) => {
                app.set_status(Some(e));
                poll_failed = true;
            }
        }
    }
}
