# hamtools user guide

This guide is the manual for `rig-tui`, the one app for the whole station (spec [#54](https://github.com/matthew-a-gordon/hamtools/issues/54)). Sections 1 to 7 cover the rig, CW, logging, configuration and satellites. Sections 8 to 11 cover the Activities, navigation, SOTA and POTA, the band plan and export, which are built and covered by tests but not yet run on the air. Terms in **bold** are defined in [`GLOSSARY.md`](../GLOSSARY.md).

Hardware caveat: only what `CLAUDE.md` lists under "Hardware status" has been run on a real radio. Everything else here is *untested on hardware*, including every SOTA, POTA, band plan and export screen.

Contents

1. [Before you start](#1-before-you-start)
2. [The rig strip](#2-the-rig-strip)
3. [Tuning and rig control](#3-tuning-and-rig-control)
4. [Sending CW](#4-sending-cw)
5. [Logging Contacts](#5-logging-contacts)
6. [Configuration](#6-configuration)
7. [Satellites](#7-satellites)
8. [The unified app](#8-the-unified-app)
9. [SOTA and POTA](#9-sota-and-pota)
10. [Band plan](#10-band-plan)
11. [Export](#11-export)
12. [Colors](#12-colors)
13. [Troubleshooting](#13-troubleshooting)

---

## 1. Before you start

`rig-tui` never touches a serial port. It talks to Hamlib's `rigctld` over TCP, and `rigctld` owns the radio. Start one for your rig first (on the author's setup these run from the NixOS config, not from this repo):

| Rig | Address | Hamlib model |
| --- | --- | --- |
| KX3 | `127.0.0.1:4532` | 2045 |
| FT-847 | `127.0.0.1:4533` | 1001 |

To try the app with no radio, run Hamlib's dummy rig:

```sh
rigctld -m 1 -t 4532
```

Run the app from the dev shell:

```sh
nix develop
cargo run -p rig-tui                      # uses the address in your config
cargo run -p rig-tui -- --rig NAME        # start on the [rig.NAME] profile from your config (a bare address is rejected)
cargo run -p rig-tui -- --config ./my.toml
```

`rig-tui` starts even when `rigctld` is down. It shows dashes and "disconnected" with the error, retries every few seconds, and reconnects by itself when the daemon returns (seen on a KX3, about 9 seconds after a restart).

On a KX3 over USB the user must be in the `dialout` group, and a login session started before joining the group does not have it.

Quit with `q` (when no input has focus) or `Ctrl-C`. Both ask first (see section 8).

## 2. The rig strip

Two rows across the top show the radio's state, polled a few times a second:

| Item | Meaning |
| --- | --- |
| Frequency | Receive frequency, in `7.070.000` form |
| Mode and passband | For example `CWR 450` |
| Step | The size of one Left/Right tuning step |
| Connection | connected, or "disconnected" with the error |
| VFO | `VFOA` or `VFOB` |
| Split | Off, or `split B 7.071.000` showing the transmit frequency |
| RX/TX | Whether the radio is receiving or transmitting (PTT, read only) |
| S-meter | `S0` to `S9`, then `S9+dB`, from the rig's `STRENGTH` reading |
| Clock | UTC |

A hint line at the bottom lists the keys that apply to the current focus. A status line above it explains refusals, such as "not connected" or "split is off".

The S-meter was seen only on a quiet band (`S0` at -54 dB). It is *untested on hardware* with a real signal.

## 3. Tuning and rig control

These are the **strip keys**. They work when no text input has focus.

| Key | Action |
| --- | --- |
| `f`, or a digit | Enter a frequency. Digits alone are kHz (`7070` is 7.070 MHz); include `.` for MHz (`7.07`). `Enter` sets it, `Esc` cancels. |
| `Left` / `Right` | Tune down / up by one step |
| `Up` / `Down` | Make the step larger / smaller: 1 Hz, 10 Hz, 100 Hz, 1 kHz (default), 10 kHz, 100 kHz |
| `m` / `M` | Next / previous mode: LSB, USB, CW, CWR, AM, FM, RTTY, RTTYR |
| `v` | Toggle VFO A and B |
| `s` | Toggle split |
| `S` | Enter the split transmit frequency (split must be on) |
| `l`, `L`, `c` | Log form, session list, config dialog (section 5 and 6) |

Things to know:

- **Mode changes change the passband on a KX3.** The app asks for passband 0 ("radio's choice"), and the KX3 applies that mode's own filter. After cycling CWR to AM to FM and back, the radio read CWR 1000 instead of the CWR 450 it started at. Restore the filter on the radio, or send `M CWR 450` to `rigctld`.
- **Split on a KX3.** Turning split on copies VFO A's frequency into VFO B, so set the transmit frequency afterwards with `S`. With split off the transmit frequency is not available, which is the KX3's behaviour, not a bug.
- Controls say why they do nothing: "not connected", "split is off", "mode not supported".

## 4. Sending CW

When the radio is in CW or CWR, a CW panel appears under the strip. It shows:

- a **transcript** of what the radio decodes (the KX3's `TB;` text, polled about four times a second). The KX3 prints `*` for characters it cannot decode, so weak or fading signals give garbage; that is the radio, not the app;
- an **input line**: type text and press `Enter` to send it as CW;
- a **Soft key** bar: six labelled slots, F1 to F6.

Focus moves with `Tab` (to the input line) and `Esc` or `Tab` (back to the strip). While the input line has focus it owns every printable key, so you can type callsigns and messages literally, and the strip keys do not fire. F1 to F6 work from either focus.

### Soft keys

Bind messages under `[macros]` in the config. Text is sent as CW when you press the key:

```toml
[macros]
F1 = "CQ CQ DE {MYCALL} {MYCALL} K"
F2 = "{THEIRCALL} DE {MYCALL} UR {MYRST} {MYRST} K"
F3 = "{THEIRCALL} TU 73 DE {MYCALL} EE"
F4 = "QRZ? DE {MYCALL}"
F5 = "PSE UR CALL AGN K"
F6 = "QRL?"
```

These are the usual CW forms: a short CQ ending in `K`, a report exchange, and a `TU 73 ... EE` close (see `docs/research/cw-message-formats.md`). `{MYCALL}` expands to your `callsign` setting (the older `{CALL}` is an error: "use {MYCALL}"); `{THEIRCALL}` and `{MYRST}` come from the Contact form. Named sets go under `[softkeys.<name>]` with the same F1 to F6 keys. Keys you leave unset use the built-in defaults. The six keys appear in reverse video under the panel: bold and underlined for a plan's own key, bold for a plan's set, dim for a built-in default, and struck through for a stored message that fails validation (it will not send).

For activations, give the plan a set of its own. `{MYREF}` is the park or summit you are on. The POTA set is based on the POTA CW guide; the SOTA phrasing (`CQ SOTA`, `S2S`, the summit reference in the exchange) has no SOTA primary source, so check it against current practice:

```toml
[softkeys.pota]
F1 = "CQ POTA {MYCALL} {MYCALL} K"
F2 = "{THEIRCALL} UR {MYRST} K"
F3 = "{THEIRCALL} TU 73 DE {MYCALL} EE"
F4 = "QRZ? POTA DE {MYCALL}"
F5 = "UR {MYRST} P2P {MYREF} {MYREF} BK"
F6 = "QRL?"

[softkeys.sota]
F1 = "CQ SOTA {MYCALL} {MYCALL} K"
F2 = "{THEIRCALL} UR {MYRST} {MYREF} K"
F3 = "{THEIRCALL} TU 73 DE {MYCALL} EE"
F4 = "QRZ? SOTA DE {MYCALL}"
F5 = "S2S {MYREF} {MYREF} K"
F6 = "QRL?"
```

Safety: the only way the app transmits is a CW send you trigger from a key press. Nothing repeats or sends on its own.

Verified on a KX3: keying CW through `rigctld-client` (confirmed on the radio display) and decoding with `TB;`. *Untested on hardware*: the same through `rig-tui`'s panel, the input line, F1-F6, text longer than the KX3's 24-character `KY` packet, and the decode polling while the panel is active.

## 5. Logging Contacts

A **Contact** is one two-way exchange with another station. The **Log** is one append-only ADIF file that holds every Contact, whatever you were doing. It defaults to `~/.local/share/hamtools/log.adi`; set `[logbook] path` to move it.

Logging needs `callsign` set in the config.

| Key | Action |
| --- | --- |
| `l`, or `Ctrl-L` | Open the Contact form |
| `L` | Show this session's Contacts |
| `Tab` / `Shift-Tab` | Next / previous field |
| `Enter` | Save the Contact |
| `Esc` | Cancel without saving |

`Ctrl-L` works from anywhere, including the CW input line. Plain `l` and `L` work only when no text input has focus.

The form asks for **Call** (required), **Mode**, **Submode**, **Grid**, **Name**, **RST sent**, **RST rcvd** and **Comment**. The app fills what the radio knows: frequency, band, mode, start time (when the form opens) and your own Locator. With split on, the frequency is the transmit frequency. If the rig has not reported frequency and mode in the last 2 seconds (or is disconnected), the form refuses to open and says why, rather than logging a stale value.

Saving writes the Contact to the Log and updates the "N logged" count in the strip. Your `station.locator` is written as `MY_GRIDSQUARE`.

Verified on a KX3 (without transmitting): the form, a saved Contact with `MY_GRIDSQUARE`, the session list, and the transmit-frequency rule under split.

## 6. Configuration

`rig-tui` reads `~/.config/hamtools/config.toml`, or the file named by `--config PATH`. Copy `config.example.toml` to start. The real config stays out of the repo.

```toml
callsign = "N0CALL"

[station]
locator = "EN71DQ"        # 6 characters; never latitude or longitude

[logbook]
path = "/home/you/.local/share/hamtools/log.adi"   # optional

[rig]
default = "kx3"

[rig.kx3]
addr = "127.0.0.1:4532"
kind = "kx3"

[macros]
F1 = "CQ CQ CQ DE {MYCALL} {MYCALL} K"
```

The old `[rig] addr` and `[kx3]` keys are no longer read: a config that still has them is rejected with a message naming the replacement (section 8, "Two rigs").

Unknown keys are kept in the file and reported as warnings at startup. They are not errors.

### The config dialog

Press `c` (or open it from the hint line) to edit the config without leaving the app. It has fields for callsign, Locator, Log path, the Active rig's address, the default rig (a profile name) and Soft keys F1 to F6. Adding or removing a rig profile is done in the TOML file.

| Key | Action |
| --- | --- |
| `Tab` / `Shift-Tab` | Move between fields |
| `Enter` on Address | Look up the typed address |
| `Ctrl-S` | Save and close |
| `Esc` | Close (asks before discarding edits) |

Saves change only the fields you edited, so your comments and ordering survive. If the file changed on disk since it was opened, the dialog shows a conflict to settle first. If the file is read-only, `Ctrl-A` applies your values for this run only. If it does not parse, saving is disabled until you fix it and press `Ctrl-R` to reload.

### Finding your Locator

Type a town in the Address field and press `Enter`. One request goes to OpenStreetMap Nominatim, and candidates appear with 6-character Locators (Middlebury, Elkhart County, gives EN71DQ). Pick one. Only the Locator is stored; the address you typed is not. A Locator is coarse by design (about 4.6 by 3.5 km) so it does not pin a home.

Verified on a KX3 setup: opening the dialog, one live lookup, picking a Locator and closing with `Ctrl-S` on an unchanged file. *Untested on hardware*: the edit-and-save, conflict, read-only and parse-error paths, and the discard prompt.

## 7. Satellites

The Satellite Activity is the satellite pass predictor and Doppler tracker for the FT-847, hosted inside `rig-tui` (the standalone `sat-tui` program is gone). Most of it is *untested on hardware*; `CLAUDE.md` lists what has been seen on the FT-847. Open it with `Ctrl-G`, then Satellite. It needs a rig with the `sat_mode` capability: on the KX3 it is listed as unavailable, with the reason. The shared top strip replaces its old title, rig line and clock; the Contact form, session list and rig connection are the app's.

```sh
cargo run -p rig-tui                          # position from station.locator, else a town-level default
cargo run -p rig-tui -- --grid EN71           # or --lat DEG --lon DEG, and --alt METRES
```

Your position comes from the command line, then `station.locator` in the shared config, then a town-level default. It is never hard-coded. Satellite settings are the `[satellite]` table of the shared config (see `config.example.toml`).

It works with no rig, as a pass viewer. TLEs (CelesTrak) and transmitters (SatNOGS) download into `~/.cache/hamtools/` when you enter the Activity and they are stale (and with `r`), and their ages are shown in its footer, so it keeps working offline.

### Reading the screen

The **pass list** shows each **Pass**: AOS, LOS, duration and peak elevation (**TCA**). A pass shorter than `short_pass_secs` (180 s by default) is marked `!`. While tracking, a live panel counts down to the tracked pass's AOS and LOS.

### Keys

The shared strip keys (`f`, `m`/`M`, `v`, `s`, `S`, Left/Right) mean what they mean everywhere. `s` is split, so the sat-mode key moved to `Z`, and `m` is the mode key, so the transmitter filter moved to `u`.

| Key | Action |
| --- | --- |
| `j` / `k`, Up / Down | Move the cursor |
| `Tab` | Switch between the pass list and the transmitter picker |
| `t` | Start or stop **tracking** |
| `[` `]` | Nudge the passband position by 1 kHz |
| `{` `}` | Nudge it by 100 Hz |
| `Z` | Switch the radio's SAT mode off (while tracking this does not hold yet, see #81) |
| `u` | Show transmitters the radio cannot demodulate |
| `r` | Refresh TLEs and transmitters |
| `l` / `L` | Log a Contact / list this session's Contacts |
| `Ctrl-C` | Quit (asks; warns that Doppler tracking is running) |

**Tracking** keeps the radio's receive and transmit frequencies on a transmitter: nominal before AOS, corrected for Doppler from AOS, and following your dial. Transmitting is always your action. It never keys the transmitter; you press PTT. Tracking runs only while Satellite is in the foreground, on the Active rig. Leaving Satellite (or switching to a rig without sat mode) stops it and the status line says so; the radio keeps whatever it was last set to. Tuning from the band plan is refused while tracking.

### Logging a satellite Contact

While tracking with a connected rig, `l` opens the shared Contact form prefilled with the satellite, the uplink mode, the frequencies last sent to the radio (the transmit frequency under split as `FREQ`, the downlink as `FREQ_RX`) and the start time. It writes to the same Log as everything else, with the usual refusals (a rig reading older than two seconds, an unknown transmit frequency under split). One RST is offered for both directions (599 when CW is worked both ways, else 59). Per-satellite `SAT_NAME` overrides go under `[satellite.names]` in the shared config.

### Open questions on the FT-847

The FT-847's CAT rate is 9600 baud here (the radio answered at 9600 and not at 4800 or 57600 on 2026-11-05). Hamlib's `ft847` backend has no SAT mode function and no read-back; turning split on turns SAT mode on, and `S 0 VFOA` (what `Z` sends) turns it off. Split and the Sub transmit side, SAT mode on and off, tracking an RS-44 transponder, the nudge keys and the Contact prefill were seen working on 2026-11-05 (see `CLAUDE.md`), with commanded frequencies read back and nothing received. `Z` while tracking does not work yet: tracking turns SAT mode back on (#81). Whether correcting both uplink and downlink is right for its SAT mode on the air is not yet checked. `v` is not implemented by the backend, so the strip shows "VFO not supported".

---

## 8. The unified app

Everything from here on is the spec in [#54](https://github.com/matthew-a-gordon/hamtools/issues/54), now built. It is covered by tests with fixtures and no live network, but none of it has been run against a radio or on the air, so treat it as *untested on hardware*.

### Activities

`rig-tui` hosts exactly one **Activity** at a time:

| Activity | What it is |
| --- | --- |
| General | Plain rig control and logging. The startup default. |
| Satellite | The former `sat-tui` screens, inside the app |
| SOTA | Activator and "chaser" **Roles** |
| POTA | Activator and Hunter **Roles** |

One shared top strip (Activity, rig, frequency and mode, UTC clock) is always visible, and each Activity fills the body below. Leaving an Activity keeps its in-memory state, and only the foreground Activity is ticked or asks the rig for anything. Leaving Satellite stops Doppler tracking and the strip says so. The radio is not retuned.

### Navigation: the go-to palette

`Ctrl-G` opens a type-to-filter palette over the current screen. There is no Home screen. Upcoming **Activation plans** come first, then the Activities, each with a one-line live state (next pass countdown, a plan's date, "needs FT-847"). `Enter` on a plan starts its Activity and Role with that plan active. An Activity the **Active rig** cannot run is shown as unavailable with the reason.

### Keys

A **text-entering area** (the CW input, a form field, the palette filter, frequency entry, `/` search) owns every printable key while focused. `Esc` and `Tab` leave it. Plain hotkeys work only when nothing is being typed. F1 to F6 work from either focus.

**Chords** work from anywhere:

| Chord | Opens |
| --- | --- |
| `Ctrl-G` | Go-to palette |
| `Ctrl-K` | Keybindings modal (context-aware, `/` filters, blocked keys greyed with a reason) |
| `Ctrl-B` | Band plan |
| `Ctrl-R` | Rig switcher |
| `Ctrl-L` | Contact form |
| `Ctrl-J` | Session list |
| `Ctrl-O` | Config |
| `Ctrl-C` | Quit confirmation |

Plain `l`, `L` and `c` remain as aliases. The strip keys in section 3 mean the same in every Activity. One change: in Satellite, the SAT-mode key moves from `s` to `Z`, so `s` is split everywhere. There is no key rebinding.

**Quitting always asks.** `Ctrl-C` works from anywhere, `q` only in General with nothing open. The prompt defaults to cancel (`n`, `Esc` or `Enter` cancel; `y` or a second `Ctrl-C` confirm). It adds a line for anything that would be lost: a Contact form with typed text, unsent CW input, or running Doppler tracking. It never refuses, and `Esc` never quits.

### Two rigs

Rigs are named profiles, and the old single-address keys are removed with no compatibility. `--rig NAME` starts on a profile other than `rig.default`:

```toml
[rig]
default = "kx3"

[rig.kx3]
addr = "127.0.0.1:4532"
kind = "kx3"

[rig.ft847]
addr = "127.0.0.1:4533"
kind = "ft847"
```

Both rigs stay connected. The inactive one is polled lightly (health, frequency and mode about every 5 seconds), so switching is instant. `Ctrl-R` opens the switcher with both connection states. Switching is refused while the current rig is transmitting (PTT on or a CW send in progress). If the new rig cannot run the current Activity, the app drops to General with a status line. Each rig keeps its own state: last frequency, mode, VFO, split, step, CW transcript and Soft key state.

What each rig can do comes from a static capability table keyed by `kind` (`kx3`, `ft847`, `generic`), not from probing Hamlib. A profile may override `cw`, `split` or `sat_mode`; an override is never counted as verified. `Ctrl-R` shows each rig's row. Rows verified on hardware are the KX3's CW sending, VFO A/B, split and its transmit frequency, and the frequency, mode, passband and S-meter reads, and the FT-847's split (seen 2026-11-04) and SAT mode on and off (seen 2026-11-05, see `CLAUDE.md`). Everything else, including the FT-847's CW, is labelled *untested on hardware*.

### Satellite settings

Satellite settings are the `[satellite]` table of the shared config (watch list, pass horizon, minimum elevation, short-pass threshold, and `[satellite.names]`); `sat-tui.toml`, its `rig_addr`, `[observer]` and `[logbook] grid` are gone. Position comes from flags, then the shared Locator, then the town-level default. Doppler commands are performed on the Active rig only while Satellite is in the foreground and the rig has the `sat_mode` capability.

### Window size

The floor is 80x24 for every Activity. Below that you see only a "Window too small" notice with the needed and current size, and keys are ignored except the quit Chord. It recovers by itself when the window grows.

Width and height add panes independently, in a fixed order, and drop them in reverse:

| Size | Adds |
| --- | --- |
| 128 columns | A second column |
| 240 columns | A third column |
| 30 rows | A band plan strip (and nearby spots on the Activator) |
| 40 rows | Key hints and a stacked today's-contacts list |

General also gains a today's-contacts pane at 128 columns or 40 rows. Satellite stays as it is. The 240-column layout is the least tested piece; the author's measured terminals are 83x47 and 172x48.

Colour uses the 16 ANSI colours only, and every coloured state also has a non-colour cue (a letter, symbol or `+`/`-`).

## 9. SOTA and POTA

### Roles

- **Activator:** you are at the summit or park, working whoever calls.
- **Hunter** (SOTA: "chaser"): you work activators from elsewhere, found through spots.

### Activation plans

An **Activation plan** is a saved intention to activate one reference on a given day: the Activity, a **Reference** list, a UTC date, intended bands and modes, a Soft key set and notes. A POTA plan can list several parks for an n-fer (the first is primary). A SOTA plan holds exactly one summit.

Plans live in `plans.toml` in the config directory, with the same safe edit handling as the config. They are created, edited, duplicated and deleted in a modal. A plan is **planned**, **active** (this session only, and back to planned after a restart) or **done** (set by hand). A plan whose date has passed stays planned and is flagged overdue. Done plans list last and can be duplicated for a repeat visit.

Saving a plan fetches that one reference once into the cache so it works offline in the field. If the lookup fails, the plan still saves, shown as unresolved. Each plan keeps its own snapshot of the reference, so wiping the cache never breaks it. Launching a plan sets your reference, loads its Soft keys and starts the **Activation tracker**.

### The Activator screen

From top to bottom: a banner (reference, name, grid, distance, UTC date, status), the tracker row, an always-visible Contact form focused on Call, the CW panel with its Send line, the Soft key row, and a recent-contacts list with Their ref and a `P`/`S` mark. POTA's banner adds a "you are spotted" line. `Esc` moves focus to the strip for retuning, and `Tab` or any printable key returns to the form.

The **Activation tracker** is derived from the Log each time: POTA counts 10 Contacts (duplicates by call, band and mode excluded), SOTA counts 4 distinct stations, for the active plan's reference and UTC day. It holds no state of its own.

With no active plan the screen shows a plan picker and still lets you log with no reference. The radio, the Log and CW never wait on a plan.

### Quick entry

The Activator Contact form shows Call, RST sent, RST received, Their ref (optional) and Comment. `Enter` on Call saves with default reports, then clears and reopens on Call. Your own reference comes from the active plan and is never typed per Contact. Filling Their ref makes the Contact park-to-park or summit-to-summit. Duplicate warnings appear at entry and never block a save. A warning appears if the UTC date rolls over mid-activation.

References are stored as standard ADIF fields (`MY_SIG`/`MY_SIG_INFO` and `SIG`/`SIG_INFO` for POTA, `MY_SOTA_REF`/`SOTA_REF` for SOTA), so any other logging program can read the Log. An n-fer is one Contact listing its parks.

### Soft key sets

Messages can contain variables, filled in when you press the key from live state:

| Variable | Meaning |
| --- | --- |
| `{MYCALL}` | Your callsign |
| `{THEIRCALL}` | The other station's call |
| `{MYREF}` | Your reference |
| `{THEIRREF}` | Their reference |
| `{MYRST}` | Report you give |
| `{THEIRRST}` | Report you receive |

If any variable has no value the send is refused with a status naming it. You never send partial text or literal braces. `{CALL}` is a hard error with "use {MYCALL}". Keys are layered per key: plan override, the plan's set, your set for the Activity, then the built-in default. The bar shows which layer each key came from. A named set that is missing falls back to the lower layers and never blocks launching. The config dialog and the plan editor share one validator that rejects unknown variables, characters the keyer cannot send and unbalanced braces, and warns past 120 characters.

### The Hunter screen

The main body is a spot list: age, call, frequency, mode, reference, distance and comment. Below it are a one-line detail row for the selected spot, the CW panel and a session count.

| Key | Action |
| --- | --- |
| `j` / `k`, arrows | Select |
| `d` | Toggle newest first / nearest first (distance hides when your position is unknown) |
| `b` / `o` / `w` | Filter by band / mode group / hide worked (session only, always shown in the title bar) |
| `Enter` | Open the Contact form from the spot (fills Call and Their ref; never tunes) |
| `t` | Tune to the spot: frequency only, mode mapped if the rig reports it, split untouched. Refused while tracking Doppler. |
| `/` | Search the cached reference list by reference or any word of the name |
| `r` | Refresh now (respects the rate limit and shows "next in Ns") |

Row markers, each with a non-colour cue: `✓` worked today (derived from the Log), a dimmed row for stale spots, `P` for a park-to-park candidate, `!` for outside your privileges.

SOTA chasing works with the spot feed off. No SOTA API call is made until the SOTA owner approves this software, so the Chaser screen shows a notice and a reference box that looks up from the offline summit list.

### Reference data and etiquette

- Park and summit data is cached in the XDG cache directory, one file per source, safe to delete, with a fetched-at time. Reference data turns a warning colour past 30 days but stays usable.
- Nothing is fetched at startup. Fetching happens on an explicit "refresh reference data", on saving a plan, or on the first lookup. Lookups read only the cache, and a missing reference shows a dim "not cached".
- Spots are kept in memory only, polled no faster than once a minute and only while a POTA Activity is in the foreground and online is enabled. They dim after 2 minutes and drop at expiry.
- All requests go through one rate-limited client: at least 1 second between any two, exponential backoff from 1 minute to 1 hour on 429 or 5xx (honouring Retry-After), 5 s connect and 15 s total timeouts, and a descriptive User-Agent with your callsign when set. Config may slow these limits and never speed them.
- Network work runs on its own task. A slow or dead network never delays a frame, a keypress, a CW send or a Log write. When offline you see one calm "offline" indicator with the last success time.

## 10. Band plan

`Ctrl-B` opens a modal for the band of the Active rig's frequency (the nearest band below when outside all bands). `Left`/`Right` or `[`/`]` step bands.

Under a frequency ruler, stacked rows show:

1. **Privilege**: what your license class permits (needs `station.license_class` in the config; with none set, the row is hidden with a note, because the app never guesses). Colour plus `+`/`-`.
2. **ARRL voluntary mode segments.**
3. **Satellite sub-bands** where they overlap.

Markers show the Active rig's frequency, and a second one for the transmit frequency when split is on.

| Key | Action |
| --- | --- |
| `Up` / `Down` | Move the cursor |
| `Enter` | Set the receive frequency only |
| `m` | Apply the segment's usual mode |
| `T` | Set the transmit frequency (split on) |
| `Esc` | Close |

The modal stays open after tuning. Tuning from it is refused while Satellite is tracking. Being out of privilege is a soft warning, never a gate: the app does not decide what you may do. 60 m is shown as discrete channels with a fixed note that the channel data is unverified. On a small screen the ARRL and satellite rows drop first. The footer shows the data's `as_of` date with a stale marker. The band plan is guidance, not a legal authority.

## 11. Export

On an Activator plan, an export action produces a file to upload by hand. It never uploads anything or changes the plan or the Log.

- **Which Contacts:** those whose reference matches the plan and whose start is on the plan's UTC date, with a toggle to include other dates for the same reference (a night activation across midnight). Contacts with no reference are never exported but are counted in the preview.
- **POTA:** ADIF with the required fields, `MY_SIG`/`MY_SIG_INFO`, `SIG`/`SIG_INFO` for park-to-park, and `MY_STATE` from the cached park. An n-fer writes one file per park, named `CALL@US-1234-YYYYMMDD.adi`. A multi-state park is one file, never one per state (a Contact would count under each). When a plan has a park in more than one state, the export preview shows a `State:` line and `s` cycles through the choices; it starts on the primary park's first state. `MY_STATE` is set from the choice and the `-US-MI` name suffix is added. A park in one state asks nothing. One choice applies to every multi-state park in the plan, and a park that does not include it exports with a warning and no `MY_STATE`. When no state is known the uploader asks. Duplicates are judged by `SUBMODE` when present.
- **SOTA:** CSV V2 with its ten fields in order, chronological, one file per activation (one per UTC day of a summit). The band column uses SOTA's names (`7MHz`) or the frequency; whether the server accepts a frequency is unverified.
- **Preview first:** file names, counts, duplicates, exclusions, warnings (too few Contacts to count, a UTC rollover, a missing required field, missing state) and the first lines. Warnings never block, and skipped Contacts are named.
- **Where it writes:** only to the export directory (default `~/hamtools-exports/`), asking before overwriting.

Exporting a Hunter log is out of scope for now.

---

## 12. Colors

Color only ever adds to something already written in text: every colored item has a character, word or position that says the same thing, so the screen works on a monochrome terminal. Only the 16 ANSI colors are used, so your terminal's theme decides the exact shades.

| Color | Meaning | Non-color cue |
| --- | --- | --- |
| Cyan | The pane that has focus (its border); the ARRL voluntary band plan row | The focused border is the one with the cursor or highlighted selection; the row is labelled `ARRL` |
| Dark gray (dim) | Secondary text: hints, placeholders, stale spots, "not cached", notes | The text itself says what it is (`~` marks a stale spot) |
| Yellow | A warning, or text that is about to be or was sent on the air; a short satellite pass; a satellite below the minimum elevation (no Doppler correction); cached data older than 30 days | The wording ("older than 30 days", "outside ... privileges"); a short pass is marked in the pass list |
| Red | An error or a refusal; being outside your license privileges in the band plan | The message names the error; the privilege row shows `-` where red and `+` where green |
| Green | Doppler tracking active, and being inside your license privileges in the band plan | The `TRACKING` heading; the `+` in the privilege row |
| Magenta | The satellite sub-band row of the band plan | The row is labelled `Sat` |
| Reverse video | The selected row of a list | The selection is also the row the keys act on |

Hunter row markers use characters, not color: `✓` worked today, `~` stale, `P` park-to-park candidate, `!` outside privileges.

---

## 13. Troubleshooting

| Symptom | Check |
| --- | --- |
| Strip shows dashes and "disconnected" | Is `rigctld` running at the configured address? The app retries by itself. |
| `Permission denied` opening the serial port | That is `rigctld`'s port, not the app's. Add the user to `dialout`, then start a fresh login (or `sg dialout -c '...'`). |
| KX3 decode shows `*` | The radio could not decode the character. Weak or fading signals do this. |
| Mode change altered the filter | The KX3 applies a per-mode filter. Restore it on the radio or with `M <mode> <passband>`. |
| "split is off" when pressing `S` | Press `s` first. |
| Logging refuses to open | Set `callsign`; make sure the rig is connected and reporting a recent frequency. |
| Config dialog will not save | Read-only file (`Ctrl-A` applies for this run), unparsable file (fix and `Ctrl-R`), or a conflict to settle first. |
| The Satellite Activity shows old data | The footer shows the TLE and transmitter ages. `r` refreshes when online. |

## Maintaining this guide

When behaviour changes, update the keys tables from the key table in the code. Keep "untested on hardware" until `CLAUDE.md`'s Hardware status lists the behaviour as verified.
