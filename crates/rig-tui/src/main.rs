use std::error::Error;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant, SystemTime};

use chrono::Utc;
use hamtools_config::Grid;
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use rig_tui::geocode::{GeocodeResult, Geocoder};
use rig_tui::refdata::ReferenceData;
use rig_tui::reference::{
    Limits, ReferenceClient, SystemClock, UreqTransport, default_cache_dir, user_agent,
};
use rig_tui::satellite::{Satellite, load_data};
use rig_tui::{
    Activator, App, Command, Config, HostEvent, PotaHunter, Rig, SotaChaser, config_path,
    default_log_path, render,
};
use sat_tui::cache::{Cache, Dataset, Decision, decide};
use sat_tui::fetch::Fetcher;
use sat_tui::observer::{Spec, resolve};
use tokio::runtime::Runtime;

/// One loop pass. The KX3 clears its decode buffer on each `TB;` and holds only 40 characters,
/// so this is also the CW panel's polling interval.
const TICK: Duration = Duration::from_millis(100);

const USAGE: &str = "usage: rig-tui [--config PATH] [--grid GRID | --lat DEG --lon DEG] [--alt METRES] [ADDR]

--offline never asks the network (POTA spots show only what is cached; satellite data is not downloaded).
ADDR is the rigctld of the Active rig; it overrides that profile's address (with no profiles: 127.0.0.1:4532).
Config defaults to $XDG_CONFIG_HOME/hamtools/config.toml (see config.example.toml).
The app starts without a rig and keeps retrying the connection.

The position the Satellite Activity predicts passes from: --grid or --lat/--lon, then station.locator
in the config, then a town-level default. Satellite settings are the [satellite] table of the config.";

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
    let mut position = Spec::default();
    let mut offline = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = |name: &str| args.next().ok_or(format!("{name} needs a value"));
        match arg.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            "--offline" => offline = true,
            "--config" => config_arg = Some(value("--config")?.into()),
            "--grid" => position.grid = Some(value("--grid")?),
            "--lat" => position.lat_deg = Some(value("--lat")?.parse()?),
            "--lon" => position.lon_deg = Some(value("--lon")?.parse()?),
            "--alt" => position.alt_m = Some(value("--alt")?.parse()?),
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
    let observer = resolve(
        &position,
        config
            .locator
            .as_deref()
            .and_then(|g| Grid::new(g).ok())
            .as_ref(),
    )?;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let mut app = App::with_callsign(config.callsign.clone())
        .with_soft_keys(config.soft_keys)
        .with_softkey_sets(config.softkey_sets)
        .with_locator(config.locator)
        .with_license_class(config.license_class)
        .with_log_path(config.log_path)
        .with_default_log_path(default_log_path(
            std::env::var("XDG_DATA_HOME").ok().as_deref(),
            std::env::var("HOME").ok().as_deref(),
        ))
        .with_rigs(&config.rigs, config.rig_default.as_deref())
        .with_config_path(config_arg.or(default_path));
    // Every reference request goes through this one client; it runs its own network thread.
    // Starting it makes no request: spots are asked for only while the POTA Hunter is in the
    // foreground (ADR 0003).
    let cache_dir = default_cache_dir().unwrap_or_else(|| std::env::temp_dir().join("hamtools"));
    let client = Arc::new(ReferenceClient::start(
        Arc::new(UreqTransport),
        Arc::new(SystemClock::new()),
        Limits::default(),
        user_agent(config.callsign.as_deref()),
        cache_dir.clone(),
    ));
    let refdata = Arc::new(ReferenceData::new(Arc::clone(&client), cache_dir));
    app = app.with_reference_data(Arc::clone(&refdata));
    app.host_activity(Box::new(PotaHunter::new(client, !offline)));
    // The SOTA Chaser reads the cached summit list only and makes no SOTA API call.
    app.host_activity(Box::new(SotaChaser::new(refdata, !offline)));
    if let Some(addr) = addr_arg {
        app = app.with_rig_addr(addr);
    }
    // The hosted Activities, after General. `App::launch_plan` finds a plan's Activity by name,
    // so the POTA Hunter (named differently) and the Activators do not clash.
    app.host_activity(Box::new(Activator::sota()));
    app.host_activity(Box::new(Activator::pota()));
    // The Satellite Activity is built from whatever the cache holds (nothing is fine). It stays
    // in the background until chosen from the go-to palette. Untested on hardware.
    let mut downloads = Downloads::new(default_cache_dir(), offline);
    let data = downloads
        .cache()
        .map(|cache| load_data(&cache))
        .unwrap_or_default();
    app.host_activity(Box::new(Satellite::new(
        data,
        observer,
        config.satellite,
        Utc::now(),
    )));
    // One connection per configured rig; both stay up.
    let mut rigs: Vec<Rig> = app.rig_addrs().into_iter().map(Rig::new).collect();
    let geocoder = Geocoder::new();

    let mut terminal = ratatui::init();
    let result = run(
        &mut terminal,
        &rt,
        &mut rigs,
        app,
        &geocoder,
        &mut downloads,
    );
    ratatui::restore();
    result
}

type Outcome = (Dataset, Result<(), String>);

/// TLE and transmitter downloads: unchanged from `sat-tui` until the shared client exists. They
/// run on a short-lived thread; the loop folds the answers back in.
struct Downloads {
    dir: Option<PathBuf>,
    tx: Sender<Outcome>,
    rx: Receiver<Outcome>,
    in_flight: usize,
    fetched: bool,
    offline: bool,
}

impl Downloads {
    fn new(dir: Option<PathBuf>, offline: bool) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            dir,
            tx,
            rx,
            in_flight: 0,
            fetched: false,
            offline,
        }
    }

    fn cache(&self) -> Option<Cache> {
        self.dir.clone().map(Cache::new)
    }

    /// Start whatever is due. `force`: the operator asked, so only the CelesTrak pause applies.
    fn refresh(&mut self, app: &mut App, force: bool) {
        if self.offline {
            if force {
                app.set_status(Some(
                    "offline: satellite data is not downloaded".to_string(),
                ));
            }
            return;
        }
        let Some(cache) = self.cache() else {
            if force {
                app.set_status(Some(
                    "no cache directory (set HOME or XDG_CACHE_HOME)".to_string(),
                ));
            }
            return;
        };
        let now = Utc::now();
        let mut wanted = Vec::new();
        for dataset in [Dataset::Tles, Dataset::Transmitters] {
            match decide(dataset, &cache.state(dataset), now, force) {
                Decision::Fetch => wanted.push(dataset),
                Decision::TooSoon { retry_in } if force => app.set_status(Some(format!(
                    "CelesTrak asks for a pause: next download allowed in {} min",
                    retry_in.num_minutes() + 1
                ))),
                Decision::TooSoon { .. } | Decision::Fresh => {}
            }
        }
        if wanted.is_empty() {
            return;
        }
        self.in_flight += wanted.len();
        app.on_host_event(HostEvent::SatelliteDownloading(true));
        app.set_status(Some("downloading satellite data...".to_string()));
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let fetcher = Fetcher::new();
            for dataset in wanted {
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

    /// Fold finished downloads into the app.
    fn poll(&mut self, app: &mut App) {
        while let Ok((dataset, outcome)) = self.rx.try_recv() {
            self.in_flight = self.in_flight.saturating_sub(1);
            match outcome {
                Ok(()) => self.fetched = true,
                Err(e) => app.set_status(Some(format!("download failed ({dataset:?}): {e}"))),
            }
            if self.in_flight == 0 {
                app.on_host_event(HostEvent::SatelliteDownloading(false));
                if std::mem::take(&mut self.fetched)
                    && let Some(cache) = self.cache()
                {
                    app.on_host_event(HostEvent::SatelliteData(Box::new(load_data(&cache))));
                    if app.status() == Some("downloading satellite data...") {
                        app.set_status(None);
                    }
                }
            }
        }
    }
}

/// Run each command against the rig and feed what it reports back to the app. Returns true when
/// the app asked to quit.
fn dispatch(
    rt: &Runtime,
    rigs: &mut [Rig],
    app: &mut App,
    geocoder: &Geocoder,
    lookups: &Sender<GeocodeResult>,
    downloads: &mut Downloads,
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
            Command::SatelliteRefresh { force } => {
                downloads.refresh(app, *force);
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
    downloads: &mut Downloads,
) -> Result<(), Box<dyn Error>> {
    // The address lookup runs on a short-lived thread; its answer comes back here, like the
    // satellite downloads, so `App` stays free of threads.
    let (lookups, answers) = mpsc::channel();
    loop {
        app.set_utc(SystemTime::now());
        let size = terminal.size()?;
        app.set_size(size.width, size.height);
        terminal.draw(|frame| render(&app, frame))?;

        if event::poll(TICK)?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            let commands = app.on_key(key);
            if dispatch(rt, rigs, &mut app, geocoder, &lookups, downloads, commands) {
                return Ok(());
            }
        }

        while let Ok(answer) = answers.try_recv() {
            app.on_geocode(answer);
        }
        downloads.poll(&mut app);

        let commands = app.tick(Instant::now());
        if dispatch(rt, rigs, &mut app, geocoder, &lookups, downloads, commands) {
            return Ok(());
        }
    }
}
