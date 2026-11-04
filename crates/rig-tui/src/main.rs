use std::error::Error;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use rig_tui::{App, Command, Config, Rig, config_path, render};
use tokio::runtime::Runtime;

/// One loop pass. The KX3 clears its decode buffer on each `TB;` and holds only 40 characters,
/// so this is also the CW panel's polling interval.
const TICK: Duration = Duration::from_millis(100);

const USAGE: &str = "usage: rig-tui [--config PATH] [ADDR]

ADDR is the rigctld to use; it overrides the config file (default 127.0.0.1:4532).
Config defaults to $XDG_CONFIG_HOME/hamtools/config.toml (see config.example.toml).
The app starts without a rig and keeps retrying the connection.";

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
    for warning in &config.warnings {
        eprintln!(
            "rig-tui: config: unknown key {} (line {})",
            warning.name, warning.line
        );
    }
    let addr = addr_arg.unwrap_or_else(|| config.rig_addr.clone());

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let mut rig = Rig::new(addr);
    let app = App::with_callsign(config.callsign).with_soft_keys(config.soft_keys);

    let mut terminal = ratatui::init();
    let result = run(&mut terminal, &rt, &mut rig, app);
    ratatui::restore();
    result
}

/// Run each command against the rig and feed what it reports back to the app. Returns true when
/// the app asked to quit.
fn dispatch(rt: &Runtime, rig: &mut Rig, app: &mut App, commands: Vec<Command>) -> bool {
    for command in commands {
        if command == Command::Quit {
            return true;
        }
        for event in rt.block_on(rig.execute(command)) {
            app.on_rig(event);
        }
    }
    false
}

fn run(
    terminal: &mut DefaultTerminal,
    rt: &Runtime,
    rig: &mut Rig,
    mut app: App,
) -> Result<(), Box<dyn Error>> {
    loop {
        terminal.draw(|frame| render(&app, frame))?;

        if event::poll(TICK)?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            let commands = app.on_key(key);
            if dispatch(rt, rig, &mut app, commands) {
                return Ok(());
            }
        }

        let commands = app.tick(Instant::now());
        if dispatch(rt, rig, &mut app, commands) {
            return Ok(());
        }
    }
}
