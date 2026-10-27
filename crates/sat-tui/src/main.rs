use std::error::Error;
use std::future::Future;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use chrono::Utc;
use orbit::{Observer, parse_tles};
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use rigctld_client::Client;
use sat_tui::app::{App, Command, Settings};
use sat_tui::cache::{Cache, Dataset, Decision, decide, tles_stale};
use sat_tui::config::{Config, config_path};
use sat_tui::fetch::Fetcher;
use sat_tui::observer::{Spec, resolve};
use sat_tui::rig::apply;
use sat_tui::satnogs::parse_transmitters;
use sat_tui::ui::{Footer, render};
use tokio::runtime::Runtime;

const USAGE: &str =
    "usage: sat-tui [--config PATH] [--rig ADDR] [--grid GRID | --lat DEG --lon DEG] [--alt METRES]

Needs no rig: without one it is a pass predictor. Position: command line, then the config file,
then a town-level default. Config defaults to $XDG_CONFIG_HOME/hamtools/sat-tui.toml.";

const TICK: Duration = Duration::from_secs(1);
const RIG_TIMEOUT: Duration = Duration::from_secs(2);
const RIG_RETRY: Duration = Duration::from_secs(5);
/// Rebuild the pass list now and then so finished passes drop off and the horizon moves on.
const REBUILD_EVERY: Duration = Duration::from_secs(30 * 60);

fn main() -> ExitCode {
    match try_main() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("sat-tui: {e}");
            ExitCode::FAILURE
        }
    }
}

struct Args {
    config: Option<PathBuf>,
    rig: Option<String>,
    observer: Spec,
}

fn parse_args() -> Result<Option<Args>, Box<dyn Error>> {
    let mut args = Args {
        config: None,
        rig: None,
        observer: Spec::default(),
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut value = |name: &str| it.next().ok_or(format!("{name} needs a value"));
        match arg.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(None);
            }
            "--config" => args.config = Some(value("--config")?.into()),
            "--rig" => args.rig = Some(value("--rig")?),
            "--grid" => args.observer.grid = Some(value("--grid")?),
            "--lat" => args.observer.lat_deg = Some(value("--lat")?.parse()?),
            "--lon" => args.observer.lon_deg = Some(value("--lon")?.parse()?),
            "--alt" => args.observer.alt_m = Some(value("--alt")?.parse()?),
            _ => return Err(format!("unknown option {arg}\n{USAGE}").into()),
        }
    }
    Ok(Some(args))
}

fn cache_dir() -> Option<PathBuf> {
    let base = match std::env::var("XDG_CACHE_HOME")
        .ok()
        .filter(|d| !d.is_empty())
    {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(std::env::var("HOME").ok()?).join(".cache"),
    };
    Some(base.join("hamtools"))
}

/// Build the app from whatever the cache holds (nothing is fine: the pass list is just empty).
fn build_app(cache: &Cache, config: &Config, observer: Observer) -> App {
    let sats = cache
        .load(Dataset::Tles)
        .ok()
        .flatten()
        .and_then(|c| parse_tles(&c.body).ok())
        .unwrap_or_default();
    let transmitters = cache
        .load(Dataset::Transmitters)
        .ok()
        .flatten()
        .and_then(|c| parse_transmitters(&c.body).ok())
        .unwrap_or_default();
    App::new(
        sats,
        transmitters,
        observer,
        Settings {
            watch: config.watch.clone(),
            horizon_hours: config.horizon_hours,
            min_elevation_deg: config.min_elevation_deg,
        },
        Utc::now(),
    )
}

/// Download `datasets` on a background thread, reporting each outcome on `tx`.
fn spawn_fetch(dir: PathBuf, datasets: Vec<Dataset>, tx: Sender<(Dataset, Result<(), String>)>) {
    std::thread::spawn(move || {
        let (cache, fetcher) = (Cache::new(dir), Fetcher::new());
        for dataset in datasets {
            let result = match dataset {
                Dataset::Tles => fetcher.tles(),
                Dataset::Transmitters => fetcher.transmitters(),
            };
            let outcome = match result {
                Ok(body) => cache
                    .store(dataset, &body, Utc::now())
                    .map_err(|e| e.to_string()),
                Err(e) => {
                    let _ = cache.record_attempt(dataset, Utc::now());
                    Err(e.to_string())
                }
            };
            if tx.send((dataset, outcome)).is_err() {
                return;
            }
        }
    });
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

struct Context {
    cache: Cache,
    cache_dir: PathBuf,
    config: Config,
    observer: Observer,
    rig_addr: String,
}

fn try_main() -> Result<(), Box<dyn Error>> {
    let Some(args) = parse_args()? else {
        return Ok(());
    };
    let default_path = config_path(
        std::env::var("XDG_CONFIG_HOME").ok().as_deref(),
        std::env::var("HOME").ok().as_deref(),
    );
    let config = Config::load(args.config.as_deref(), default_path.as_deref())?;
    let observer = resolve(&args.observer, &config.observer)?;
    let cache_dir =
        cache_dir().ok_or("cannot find a cache directory (set HOME or XDG_CACHE_HOME)")?;
    let rig_addr = args.rig.clone().unwrap_or_else(|| config.rig_addr.clone());
    let ctx = Context {
        cache: Cache::new(cache_dir.clone()),
        cache_dir,
        config,
        observer,
        rig_addr,
    };

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let mut terminal = ratatui::init();
    let result = run(&mut terminal, &rt, &ctx);
    ratatui::restore();
    result
}

fn run(terminal: &mut DefaultTerminal, rt: &Runtime, ctx: &Context) -> Result<(), Box<dyn Error>> {
    let mut app = build_app(&ctx.cache, &ctx.config, ctx.observer);
    let (tx, rx): (_, Receiver<_>) = mpsc::channel();
    let mut fetching = 0usize;
    let mut rebuild_pending = false;
    let mut last_rebuild = Instant::now();

    let auto: Vec<Dataset> = [Dataset::Tles, Dataset::Transmitters]
        .into_iter()
        .filter(|&d| decide(d, &ctx.cache.state(d), Utc::now(), false) == Decision::Fetch)
        .collect();
    if !auto.is_empty() {
        fetching += auto.len();
        app.set_status(Some("downloading satellite data...".to_string()));
        spawn_fetch(ctx.cache_dir.clone(), auto, tx.clone());
    }

    let mut client: Option<Client> = None;
    let mut rig_status = "not connected".to_string();
    let mut last_connect = Instant::now() - RIG_RETRY;
    let mut last_tick = Instant::now() - TICK;
    let try_connect = |client: &mut Option<Client>, status: &mut String| match with_timeout(
        rt,
        Client::connect(ctx.rig_addr.as_str()),
    ) {
        Ok(c) => {
            *client = Some(c);
            *status = format!("connected {}", ctx.rig_addr);
        }
        Err(e) => *status = format!("not connected ({e})"),
    };
    try_connect(&mut client, &mut rig_status);

    loop {
        let now = Utc::now();
        let (tle, sat) = (
            ctx.cache.state(Dataset::Tles),
            ctx.cache.state(Dataset::Transmitters),
        );
        let footer = Footer {
            now,
            tle_age: tle.fetched_at.map(|t| now - t),
            transmitter_age: sat.fetched_at.map(|t| now - t),
            tles_stale: tle.fetched_at.is_some_and(|t| tles_stale(t, now)),
            rig: rig_status.clone(),
        };
        terminal.draw(|frame| render(&app, &footer, frame))?;

        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            match app.handle_key(key) {
                Some(Command::Quit) => return Ok(()),
                Some(Command::Refresh) => {
                    let mut wanted = Vec::new();
                    for dataset in [Dataset::Tles, Dataset::Transmitters] {
                        match decide(dataset, &ctx.cache.state(dataset), now, true) {
                            Decision::Fetch => wanted.push(dataset),
                            Decision::TooSoon { retry_in } => app.set_status(Some(format!(
                                "CelesTrak asks for a pause: next download allowed in {} min",
                                retry_in.num_minutes() + 1
                            ))),
                            Decision::Fresh => {}
                        }
                    }
                    if !wanted.is_empty() {
                        fetching += wanted.len();
                        app.set_status(Some("downloading satellite data...".to_string()));
                        spawn_fetch(ctx.cache_dir.clone(), wanted, tx.clone());
                    }
                }
                None => {}
            }
            // Pressing track with no rig is the moment to try again.
            if app.tracking().is_some()
                && client.is_none()
                && last_connect.elapsed() > Duration::from_millis(500)
            {
                last_connect = Instant::now();
                try_connect(&mut client, &mut rig_status);
                if client.is_some() {
                    app.resync();
                }
            }
        }

        while let Ok((dataset, outcome)) = rx.try_recv() {
            fetching = fetching.saturating_sub(1);
            match outcome {
                Ok(()) => rebuild_pending = true,
                Err(e) => app.set_status(Some(format!("download failed ({dataset:?}): {e}"))),
            }
            if fetching == 0
                && rebuild_pending
                && app.status() == Some("downloading satellite data...")
            {
                app.set_status(None);
            }
        }
        let due = last_rebuild.elapsed() > REBUILD_EVERY;
        if (rebuild_pending || due) && app.tracking().is_none() {
            app = build_app(&ctx.cache, &ctx.config, ctx.observer);
            rebuild_pending = false;
            last_rebuild = Instant::now();
        }

        if last_tick.elapsed() >= TICK {
            last_tick = Instant::now();
            if client.is_none() && app.tracking().is_some() && last_connect.elapsed() >= RIG_RETRY {
                last_connect = Instant::now();
                try_connect(&mut client, &mut rig_status);
                if client.is_some() {
                    app.resync();
                }
            }
            if let Some(rig) = &mut client {
                match app.tick(now) {
                    Ok(commands) if !commands.is_empty() => {
                        if let Err(e) = with_timeout(rt, apply(rig, &commands)) {
                            app.set_status(Some(format!("rig: {e}")));
                            rig_status = format!("lost ({e})");
                            client = None;
                        }
                    }
                    Ok(_) => {}
                    Err(e) => app.set_status(Some(format!("tracking: {e}"))),
                }
            }
        }
    }
}
