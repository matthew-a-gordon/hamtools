# hamtools

A terminal app for running a ham radio from a small computer: rig control, CW, logging, and the specific kinds of radio play I actually do (SOTA, POTA and satellites). It is a TUI client for Hamlib's `rigctld`, written in Rust with ratatui.

## Why this exists

I have two radios and they get used very differently.

- An **Elecraft KX3** goes with me backcountry: wandering, and the occasional Summits on the Air (SOTA) activation.
- A **Yaesu FT-847** is the one for playing with satellites, and for overlanding and camping.

I also have a Clockwork Pi that looked like a good partner for the KX3: small enough to carry, with room to control the radio, log contacts and send CW from one place. But it has a tiny screen, an awful trackball, and it suits text-mode programs far better than graphical ones. So the answer was a TUI: keyboard driven, laid out for 80x24 and nothing more, and quiet about the network.

Once a rig-control TUI existed, extending it to satellite work was the natural next step. The result is `rig-tui`: a TUI `rigctld` client for rig control, with **Activities** for specific ways of playing with the radio. Today those are General (plain rig control and logging), Satellite, SOTA and POTA. Each Activity is a screen built around one job, so a SOTA activation looks and behaves differently from a satellite pass, while the rig strip across the top stays the same.

It started as separate tools (a KX3 CW sender, then `sat-tui`) and was folded into one app. The plan and the reasons are in spec [#54](https://github.com/matthew-a-gordon/hamtools/issues/54) and in [`docs/adr/`](docs/adr).

## What it does

**Rig control (every Activity).** A two-row strip shows frequency, mode and passband, step, connection state, your location, VFO, split with the transmit frequency, RX/TX, S-meter and a UTC clock. You can tune by key or direct entry, cycle modes, switch VFO, and set split. The app starts with no rig, shows "disconnected" with the error, and reconnects by itself.

**Location from GPS.** If `gpsd` has a GPS (for example the ClockworkPi uConsole's HackerGadgets AIO v2 board), the strip shows your Locator and the GPS coordinates, and `Ctrl-P` fills the Locator from the fix: in the config dialog to save it, anywhere else for this session only. Only the 6-character Locator is ever saved.

**Two rigs at once.** Rigs are named profiles in the config (`[rig.kx3]`, `[rig.ft847]`). Both stay connected, `Ctrl-R` switches between them (refused while one is transmitting), and `--rig NAME` picks the one to start on. A static capability table decides what each rig can run, rather than asking `rigctld` what it claims.

**CW.** A panel shows what the radio decodes and has an input line to send. Six Soft keys (F1 to F6) hold canned messages with variables such as `{MYCALL}`, `{THEIRCALL}`, `{MYRST}` and `{MYREF}`, filled in at the moment you press the key. Named sets (for example `[softkeys.pota]` and `[softkeys.sota]`) are picked per activation plan. See `config.example.toml`.

**Logging.** A Contact form is prefilled from the radio (frequency, mode, band, time, your Locator). Contacts go to one append-only ADIF Log that any other logging program can read. With split on, the logged frequency is the transmit frequency.

**Satellite.** Pass prediction from CelesTrak TLEs, transmitters from SatNOGS, and Doppler-corrected tracking for the FT-847's satellite mode, with a live AOS/LOS countdown and passband nudging. It works with no rig as a pass viewer, and offline from its cache. It tunes the radio but never keys it; you press PTT.

**SOTA and POTA.** Activation plans are saved intentions to activate a summit or one or more parks (n-fers included), with bands, modes and a Soft key set. As an *activator* you get an always-on quick-entry Contact form, a tracker (POTA: 10 Contacts; SOTA: 4 distinct stations) and a recent-contacts list. As a *hunter* you get a filtered POTA spot list, and for SOTA an offline summit lookup. Plans export to POTA ADIF (one file per park) and SOTA CSV. Files are written locally and never uploaded.

**Band plan.** `Ctrl-B` shows privilege segments for your license class, the ARRL voluntary plan and satellite sub-bands, with tune-from-the-plan. Privilege is a soft warning, never a gate. It is not a compliance tool: check the rules and your licence.

**Small screens.** It works from 80x24 and adds panes at wider and taller sizes. Below 80x24 it says "Window too small" rather than drawing garbage.

Navigation is `Ctrl-G` (a go-to palette that lists upcoming plans and Activities), `Ctrl-K` (context-aware keybindings, so you never have to remember), and the other chords `Ctrl-B`, `Ctrl-R`, `Ctrl-L`, `Ctrl-J`, `Ctrl-O`, `Ctrl-P`. Quitting always asks, and the prompt defaults to cancel.

## What I have actually tested

Short version: the KX3 side is in decent shape and the FT-847 side is mostly not. Every result is recorded, with dates and setups, in [`CLAUDE.md`](CLAUDE.md) under "Hardware status", and anything not listed there is **untested on hardware**.

- **KX3:** rig control through `rigctld` (tuning, VFO, split, mode cycling, S-meter read), sending CW and decoding with `TB;`, logging, the config dialog and address lookup have all run against the radio. `rig-tui`'s CW panel and Soft keys have not yet.
- **FT-847:** the strip, the Satellite Activity's pass list and transmitter list, and Doppler-corrected tracking were driven against the radio with commanded frequencies read back. Nothing was received (no antenna was connected) and nothing was transmitted. Satellite-mode engagement, the passband nudge keys and a real signal on the S-meter are still untested.
- **GPS:** the uConsole's AIO v2 GPS through `gpsd`, with a 3D fix: the strip, `Ctrl-P` and saving the Locator. Losing the fix while running is untested.
- **Never tested:** PTT while transmitting, and SOTA and POTA on the air. The reference-data fetching and the export files are covered by tests with fixtures, not by a real activation.

## Safety and good manners

- Tools never open serial ports. Only `rigctld` does; everything here is a TCP client.
- Nothing transmits except an explicit CW send from a key press. Nothing repeats or sends on its own.
- Reference data (parks, summits, spots) is cached on disk and fetched only through one rate-limited client; nothing is fetched at startup, and `--offline` turns all of it off.
- The SOTA API's terms bar AI-generated software without the owner's approval. I designed this software and made its design decisions myself, but I used AI coding assistants to help build it, so I am not assuming the terms don't apply. No SOTA API call is made until the owner approves. The published summit list file is a download, not an API call, and is allowed under the same rule ([ADR 0003](docs/adr/0003-reference-data-is-offline-first-behind-one-client.md)).
- Your position is runtime input (config, flags, `gpsd`), never hard-coded. An address lookup turns a town into a Locator with one request when you ask, and the address is not stored. The GPS is read through `gpsd` over TCP, not its serial port, and its coordinates are shown but never stored.

## Install and run

```sh
nix develop            # or `direnv allow`: toolchain and hamlib_4
cargo run -p rig-tui   # control the rig at 127.0.0.1:4532; Ctrl-G then Satellite for passes
cargo run -p rig-tui -- --rig ft847   # start on a named rig profile
```

`nix build` produces `packages.<system>.default` (alias `hamtools`) containing all binaries.

`rigctld` owns each serial port. Run one per rig (managed by your own system config; this repo does not start them). For development and tests, a dummy rig:

```sh
rigctld -m 1 -t 4532
```

| Rig | Address | Hamlib model |
| --- | --- | --- |
| KX3 | `127.0.0.1:4532` | 2045 |
| FT-847 | `127.0.0.1:4533` | 1001 |

Copy `config.example.toml` to `~/.config/hamtools/config.toml` and set your callsign, Locator and rig profiles. The config is edited in place from the app (`Ctrl-O`), and a field-level save keeps your comments and unknown keys. `--offline`, `--grid`, `--gpsd` and `--no-gps` are in `rig-tui --help`.

### On the ClockworkPi (or any machine you reach over ssh)

`rig-tui` does not start `rigctld`; it is a TCP client of one daemon per rig. On a machine you reach over ssh, run those daemons as systemd units bound to each radio's device by a udev rule: they survive ssh drops and `rig-tui` restarts, start when a radio is plugged in, and `rig-tui` reconnects by itself. With no radio plugged in the strip shows `disconnected` and a yellow `⚠ cannot reach rigctld ... connection refused` line, which is correct and stays until a daemon is reachable; if another rig is connected the line says so and points at `Ctrl-R`. The recipe (udev rule, a unit per rig, install and read-only checks) is in the [user guide](docs/user-guide.md), "Under systemd (the ClockworkPi)", and templates are in [`docs/systemd/`](docs/systemd). It is **untested on hardware**: not yet tried on the ClockworkPi, whose hamlib is probably 4.6.x.

## The crates

| Crate | Purpose |
| --- | --- |
| `rig-tui` | The app: rig strip, Activities, CW panel and Soft keys, logging, plans, band plan, export, config dialog |
| `sat-tui` | Library (no binary): TLE and SatNOGS downloads, the cache, the observer position and the Doppler tracker. The screens are the Satellite Activity in `rig-tui` |
| `rigctld-client` | Async client for the rigctld text protocol |
| `logbook` | Contact, callsign and Locator types; ADIF writer; the append-only Log |
| `orbit` | Orbit propagation for pass prediction and Doppler |
| `hamtools-config` | The shared `config.toml`: typed read, field-level write; bundled band plan data |
| `hamtools-tui` | Shared modal, text-field and list overlay widgets |

## Not built, on purpose

Posting or self-spotting, uploading anything, band plans outside the US, key rebinding, and reading or writing the KX3's own message buffers (tracked separately as ideas for a future KX3 Utility replacement).

## Documentation

- [User guide](docs/user-guide.md): running `rig-tui`, keys and configuration.
- [`GLOSSARY.md`](GLOSSARY.md): the project's terms (Contact, Log, Locator, Activity, Soft key and so on).
- [`docs/adr/`](docs/adr): architecture decisions.
- [`docs/research/`](docs/research): notes on band plans, reference data and CW message formats.
- [`CLAUDE.md`](CLAUDE.md): conventions, boundaries and the hardware status list.
- `config.example.toml`: the annotated config.

## Development

```sh
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```
