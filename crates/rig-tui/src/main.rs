use std::error::Error;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::mpsc::{self, Sender};
use std::time::{Duration, Instant, SystemTime};

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use rig_tui::geocode::{GeocodeResult, Geocoder};
use rig_tui::{App, Command, Config, Rig, config_path, default_log_path, render};
use tokio::runtime::Runtime;

/// One loop pass. The KX3 clears its decode buffer on each `TB;` and holds only 40 characters,
/// so this is also the CW panel's polling interval.
const TICK: Duration = Duration::from_millis(100);

const USAGE: &str = "usage: rig-tui [--config PATH] [ADDR]

ADDR is the rigctld of the Active rig; it overrides that profile's address (with no profiles: 127.0.0.1:4532).
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

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let mut app = App::with_callsign(config.callsign)
        .with_soft_keys(config.soft_keys)
        .with_locator(config.locator)
        .with_log_path(config.log_path)
        .with_default_log_path(default_log_path(
            std::env::var("XDG_DATA_HOME").ok().as_deref(),
            std::env::var("HOME").ok().as_deref(),
        ))
        .with_rigs(&config.rigs, config.rig_default.as_deref())
        .with_config_path(config_arg.or(default_path));
    if let Some(addr) = addr_arg {
        app = app.with_rig_addr(addr);
    }
    // One connection per configured rig; both stay up.
    let mut rigs: Vec<Rig> = app.rig_addrs().into_iter().map(Rig::new).collect();
    let geocoder = Geocoder::new();

    let mut terminal = ratatui::init();
    let result = run(&mut terminal, &rt, &mut rigs, app, &geocoder);
    ratatui::restore();
    result
}

/// Run each command against the rig and feed what it reports back to the app. Returns true when
/// the app asked to quit.
fn dispatch(
    rt: &Runtime,
    rigs: &mut [Rig],
    app: &mut App,
    geocoder: &Geocoder,
    lookups: &Sender<GeocodeResult>,
    commands: Vec<Command>,
) -> bool {
    for command in commands {
        match &command {
            Command::Quit => return true,
            Command::Lookup(address) => {
                geocoder.lookup_in_background(address.clone(), lookups.clone());
                continue;
            }
            Command::ClearGeocodeCache => {
                geocoder.clear_cache();
                continue;
            }
            _ => {}
        }
        // Ordinary commands go to the Active rig; a light poll and an address change name
        // their rig.
        let target = match &command {
            Command::LightPoll(rig) | Command::SetRigAddr { rig, .. } => *rig,
            _ => app.active_rig(),
        };
        let background = matches!(command, Command::LightPoll(_));
        let Some(rig) = rigs.get_mut(target) else {
            continue;
        };
        for event in rt.block_on(rig.execute(command)) {
            if background {
                app.on_rig_at(target, event);
            } else {
                app.on_rig(event);
            }
        }
    }
    false
}

fn run(
    terminal: &mut DefaultTerminal,
    rt: &Runtime,
    rigs: &mut [Rig],
    mut app: App,
    geocoder: &Geocoder,
) -> Result<(), Box<dyn Error>> {
    // The address lookup runs on a short-lived thread; its answer comes back here, like sat-tui's
    // fetcher, so `App` stays free of threads.
    let (lookups, answers) = mpsc::channel();
    loop {
        app.set_utc(SystemTime::now());
        terminal.draw(|frame| render(&app, frame))?;

        if event::poll(TICK)?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            let commands = app.on_key(key);
            if dispatch(rt, rigs, &mut app, geocoder, &lookups, commands) {
                return Ok(());
            }
        }

        while let Ok(answer) = answers.try_recv() {
            app.on_geocode(answer);
        }

        let commands = app.tick(Instant::now());
        if dispatch(rt, rigs, &mut app, geocoder, &lookups, commands) {
            return Ok(());
        }
    }
}
