# hamtools

Rust TUIs (ratatui) for ham rig control and satellite work. Targets an Elecraft KX3 and a Yaesu FT-847, both driven through Hamlib's `rigctld`.

| Crate | Purpose |
| --- | --- |
| `rigctld-client` | Async client for the rigctld text protocol |
| `rig-tui` | Rig control TUI for any rigctld: rig strip, CW panel with Soft keys (F1-F6), logging, config dialog |
| `hamtools-config` | The shared `config.toml`: typed read, field-level write |
| `hamtools-tui` | Shared modal, text-field and list overlay widgets |
| `sat-tui` | Pass prediction, Doppler-corrected rig tracking, SatNOGS transmitter data |

Status: early. `rig-tui`'s strip, tuning, VFO and split, logging and config dialog have been run against a KX3 without transmitting; nothing else has been tried on real hardware. `CLAUDE.md` ("Hardware status") lists exactly what is verified.

## Development

```sh
nix develop            # or `direnv allow`
rigctld -m 1 -t 4532   # dummy rig for tests and development
cargo test --workspace
```

`nix build` produces `packages.<system>.default` (alias `hamtools`) containing all binaries.

## Architecture

`rigctld` owns each serial port; every tool here is a TCP client. KX3 is expected at `127.0.0.1:4532` (hamlib model 2045) and FT-847 at `127.0.0.1:4533` (model 1001).

## Configuration

`rig-tui` reads `~/.config/hamtools/config.toml` (or `--config PATH`; `[kx3] addr` is still read as `[rig] addr`); see `config.example.toml`. Set `callsign` once and type `{call}` in text you send, e.g. `CQ CQ DE {call} K`. The real config stays out of the repo. It starts even when `rigctld` is down and retries every few seconds.
Bind `F1` to `F6` under `[macros]` to send canned messages with one key; they show as a reverse-video Soft key bar under the CW panel (CW and CWR only) and work from either focus. `Tab` moves focus between the strip and the input line, `Esc` returns to the strip, `q` or `Ctrl-C` quits.

## sat-tui

Satellite pass predictor and Doppler-corrected tracker for the FT-847's satellite mode. Works with no rig attached (pass viewer). Downloads TLEs (CelesTrak) and transmitters (SatNOGS) into `~/.cache/hamtools/` and shows their age, so it keeps working offline.

```sh
sat-tui                       # position from sat-tui.toml, else a town-level default
sat-tui --grid EN71 --rig 127.0.0.1:4533
```

The pass list shows each pass's start, end, length and peak elevation; passes shorter than `short_pass_secs` (default 180) are marked `!`. While tracking, the live panel counts down to the tracked pass's AOS and LOS.

Keys: `j`/`k` or arrows move, `Tab` switches pass list and transmitter picker, `t` starts or stops tracking, `[` `]` nudge the passband position by 1 kHz and `{` `}` by 100 Hz, `s` switches the radio's SAT mode off, `m` shows transmitters the radio cannot demodulate, `r` refreshes data, `q` quits. It never keys the transmitter; you press PTT. See `sat-tui.example.toml`.

### Logging Contacts (untested on hardware)

While tracking with a connected rig, `l` opens a form prefilled with the satellite, modes, the frequencies last sent to the radio (uplink as `FREQ`, downlink as `FREQ_RX`) and the start time. Type the other station's call (required), then Enter saves it to the Log, an append-only ADI file; Esc cancels, Tab and Shift-Tab move between fields. `L` lists this session's Contacts. Set `callsign` in the shared `config.toml` to enable it; see `sat-tui.example.toml` for the `[logbook]` section and `--log PATH`. Nothing here can key the transmitter.
