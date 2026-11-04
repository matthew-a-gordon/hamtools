# hamtools

Terminal tools (Rust, ratatui) for ham rig control, logging, CW and satellite work. They target an Elecraft KX3 and a Yaesu FT-847, both driven through Hamlib's `rigctld`.

**Direction:** `rig-tui` is becoming the one TUI for the whole station. Spec [#54](https://github.com/matthew-a-gordon/hamtools/issues/54) folds `sat-tui` into it as the Satellite Activity and adds SOTA and POTA Activities, a band plan, activation plans and log export. This README separates what works today from what is planned. The [user guide](docs/user-guide.md) follows the same split.

| Crate | Purpose |
| --- | --- |
| `rig-tui` | Rig control TUI for any rigctld: rig strip, CW panel with Soft keys (F1-F6), logging, config dialog |
| `sat-tui` | Pass prediction, Doppler-corrected rig tracking, SatNOGS transmitter data (to be folded into `rig-tui`) |
| `rigctld-client` | Async client for the rigctld text protocol |
| `logbook` | Contact, callsign and Locator types; ADIF writer; the append-only Log |
| `orbit` | Orbit propagation for pass prediction and Doppler |
| `hamtools-config` | The shared `config.toml`: typed read, field-level write |
| `hamtools-tui` | Shared modal, text-field and list overlay widgets |

## Status

Early. Hardware results are recorded in `CLAUDE.md` ("Hardware status"); anything not listed there is **untested on hardware**.

- Run against a KX3 without transmitting: `rig-tui`'s strip, tuning, VFO and split, mode cycling, logging, config dialog and address lookup.
- Verified on a KX3 through `rigctld-client`: sending CW and decoding with `TB;`. `rig-tui`'s CW panel and Soft keys are not yet run on hardware.
- Untested on hardware: all of `sat-tui`, every FT-847 behaviour, the S-meter with a real signal, and PTT while transmitting.

## Features today

### Rig control (`rig-tui`)

- **Rig strip:** frequency, mode and passband, tuning step, connection state, VFO, split with the transmit frequency, RX/TX, S-meter and a UTC clock.
- **Tuning:** direct frequency entry, stepping from 1 Hz to 100 kHz, mode cycling (LSB, USB, CW, CWR, AM, FM, RTTY, RTTYR), VFO A/B, split on/off and a split transmit frequency.
- **CW panel** (in CW and CWR): a transcript of what the radio decodes, an input line to send, and six Soft keys (F1-F6) bound to canned messages. `{call}` expands to your callsign.
- **Logging:** a Contact form prefilled from the radio (frequency, mode, band, start time, your Locator), a count of Contacts logged this session and a session list. Contacts go to one append-only ADIF Log. With split on, the form's frequency is the transmit frequency.
- **Config dialog:** edit the callsign, Locator, Log path, rig address and Soft keys in place, with a safe field-level save that keeps your comments and unknown keys. An address lookup (OpenStreetMap Nominatim, one request when you ask) turns a town into a 6-character Locator. The address is never stored.
- **Resilient:** starts with no rig, shows "disconnected" with the error, and reconnects on its own.

### Satellites (`sat-tui`, untested on hardware)

- Pass prediction from CelesTrak TLEs, with start, end, length and peak elevation, and short passes flagged.
- Transmitters from SatNOGS, with a picker that hides what the radio cannot demodulate.
- Doppler-corrected tracking of the FT-847's satellite mode, with a live AOS/LOS countdown and passband nudging.
- Works with no rig as a pass viewer, and offline from the cache in `~/.cache/hamtools/` with data ages shown.
- Contact logging that records satellite name, uplink and downlink.

### Safety

Tools never open serial ports; only `rigctld` does. Nothing can key the transmitter except an explicit CW send from a key press. `sat-tui` tunes but never keys: you press PTT.

## Planned (spec #54, not built yet)

Seventeen build slices, tracked as issues #55-#71.

- **One app, several Activities:** General (today's `rig-tui`, the startup default), Satellite (today's `sat-tui` screens), SOTA and POTA, each with an Activator and a Hunter Role (SOTA calls the second one "chaser"). One shared top strip is always visible.
- **Navigation and keys:** a Ctrl-G go-to palette, Ctrl-K context-aware keybindings modal, Chords for every overlay (Ctrl-B bandplan, Ctrl-R rig switcher, Ctrl-L log form, Ctrl-J session list, Ctrl-O config), and a quit confirmation that defaults to cancel.
- **Two rigs:** named profiles (`[rig.kx3]`, `[rig.ft847]`), both kept connected, switchable from anywhere and refused while transmitting. A static capability table decides what each rig can run.
- **Band plan modal:** privilege segments by license class, the ARRL voluntary plan and satellite sub-bands, one band at a time, with tune-from-the-plan. Privilege is a soft warning, never a gate.
- **Activation plans:** saved intentions to activate a summit or park(s), including n-fers, with bands, modes and a Soft key set. Launching one sets your reference and starts the tracker.
- **Activator:** an always-on quick-entry Contact form, an Activation tracker (POTA 10 Contacts, SOTA 4 distinct stations) and a recent-contacts list.
- **Hunter:** a filtered spot list from the POTA feed, an offline summit lookup for SOTA, and a Contact form opened from a spot.
- **Soft key sets:** variables such as `{MYCALL}`, `{THEIRCALL}`, `{MYREF}` and `{THEIRRST}` filled in at press time, layered from plan to built-in defaults.
- **Export:** POTA ADIF (one file per park) and SOTA CSV, with a preview. Files are written to a local directory and never uploaded.
- **Sizes:** works from 80x24 up, adding panes at 128 and 240 columns and 30 and 40 rows. Below 80x24 it shows a "Window too small" notice.
- **Politeness:** offline-first, with reference data cached on disk and fetched only through one rate-limited client. No SOTA API call is made until the SOTA owner approves this software.

Out of scope for now: posting or self-spotting, uploading anything, band plans outside the US, key rebinding, and reading or writing the KX3's own message buffers.

## Install and run

```sh
nix develop            # or `direnv allow`: toolchain and hamlib_4
cargo run -p rig-tui   # control the rig at 127.0.0.1:4532
cargo run -p sat-tui   # pass viewer / tracker
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

## Documentation

- [User guide](docs/user-guide.md): running `rig-tui` and `sat-tui`, keys, configuration, and what is coming.
- [`GLOSSARY.md`](GLOSSARY.md): the project's terms (Contact, Log, Locator, Activity, Soft key and so on).
- [`docs/adr/`](docs/adr): architecture decisions behind the planned unified app.
- [`CLAUDE.md`](CLAUDE.md): conventions, boundaries and the hardware status list.
- `config.example.toml`, `sat-tui.example.toml`: annotated configs.

## Development

```sh
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```
