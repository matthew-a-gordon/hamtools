# hamtools

Cargo workspace: `rigctld-client`, `rig-tui`, `sat-tui` (library only, used by `rig-tui`; no binary), `logbook`, `orbit`, `hamtools-config`, `hamtools-tui`. Keep dependencies minimal; justify heavy crates.

## Commands
- `nix develop` (or direnv) for the toolchain and `hamlib_4`
- `rigctld -m 1 -t 4532` runs the dummy rig for tests
- `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
- `nix build` builds the packaged binaries (needs `Cargo.lock` committed)
- Drive a TUI with `herdr pane` (`split`, `run`, `send-keys`, `read`, `wait-output`, `close`; `herdr --skill` prints its agent guide). `tmux` is not installed. `nc` needs `-N`, not `-q1`. The shell is zsh, so there is no bash `PIPESTATUS`.

## Boundaries
- Tools never open serial ports. Only `rigctld` does; everything else is a TCP client.
- KX3: `127.0.0.1:4532`, model 2045. FT-847: `127.0.0.1:4533`, model 1001 (not 1038). The daemons are managed by the user's NixOS config, not this repo.
- Observer position is runtime input (config, flags, later gpsd), never hard-coded. Defaults and fixtures use town-level Middlebury, IN only (~41.67 N, 85.71 W, EN71); no street address in the repo.
- Styling comes from `hamtools_tui::theme` (ADR 0006): no literal `Color::` or `Modifier::REVERSED` for selection at a call site; add a word to the theme instead.

## Hardware status (say "untested on hardware" for anything not listed as verified)
Before running anything against a radio, read `docs/agents/hardware-session.md`.

Verified on the user's KX3 (2026-10-26, hamlib 4.7.2, rigctld model 2045):
- 38400 baud works on `/dev/kx3`. The user must be in `dialout` (a login session started before joining the group lacks it; use `sg dialout -c '...'`).
- `w TB;` through rigctld returns the documented `TBtrrs;` reply (`Reply: TB006F C E ;\0RPRT 0`; `rigctld-client` handles the framing). The KX3's decoder emits `*` for undecodable characters, so weak or fading signals give garbage; that is the radio, not the client.
- `b <text>` (`send_morse`) keys CW on the KX3; the user confirmed it on the radio display. `TB;`'s transmit count `t` drains as it sends.
- Raw `rigctld` protocol on the KX3 (2026-11-04, hamlib 4.7.2, model 2045, `-s 38400`, a temporary daemon run by the agent), checked by hand over TCP, not through `rigctld-client`: `f`, `m` (`CWR` with passband `400`), `v` (`VFOA`), `s`, `t` (PTT, read only) and `l STRENGTH` (a signed dB value relative to S9, `-54` on a quiet band) all read back. `V VFOB`/`V VFOA`, `S 1 VFOB`/`S 0 VFOA`, `F` and `M` all returned `RPRT 0` and read back correctly, then were restored. Nothing was transmitted.
- Split TX frequency (2026-11-04, same setup as above, raw `rigctld` over TCP, nothing transmitted, state restored): `i` is correct. `S 1 VFOB` makes the KX3 copy VFO A's frequency into VFO B (raw `FB;` read 7071000 before, 7069920 after), which is why `i` once seemed to return A's frequency. With split on, setting B afterwards (`V VFOB`, `F`) makes `i` return it (7071000, matching `FB;`), while `f` keeps returning the receive frequency. `FT1;` reads back with split on, `FT0;` with it off. With split off, `i` returns 0 and `x` returns an empty mode.
- `rig-tui` against the KX3 (2026-11-04, `rig-tui` from `main` at 6ff6fc1, a temporary `rigctld` model 2045 on 4532 started by the agent, driven in tmux; nothing transmitted, no CW keys pressed, radio state restored afterwards; tests that wrote files used a scratch config and Log): the two-row strip (frequency, mode and passband, step, connection, VFO, split with transmit frequency, RX, S-meter, UTC clock); `v` (VFO A/B), `s` (split on/off), `S` (split transmit frequency: strip showed `split B 7.071.000` and `FB;` read 7071000), Left/Right tuning (1 kHz steps), `f` then `7070` Enter (read back 7.070.000), and `m`/`M` mode cycling (CWR, AM, FM and back, each read back); stopping `rigctld` while `rig-tui` ran gave dashes and "disconnected" with the error, and restarting it reconnected on its own (seen at the next look, 9 s after the restart); the log form (`l`, frequency, mode CW, RST 599/599, saved Contact written to the Log with `MY_GRIDSQUARE` from the Locator, count "1 logged", `L` session list); with split on the form's `FREQ` was the transmit frequency (7.071000); the config dialog (`c`) opened, with `Ctrl-S` closing it; address lookup (one live Nominatim request, "Middlebury, Indiana", town-level) returned candidates with 6-character Locators (EN71DQ for Middlebury, Elkhart County), and the address was not stored. The S-meter showed `S0` for the quiet band's `STRENGTH` of -54.
- Mode changes on the KX3 change the passband: `rig-tui`'s mode set sends passband 0, and after cycling CWR to AM to FM and back the radio read CWR 1000 instead of the CWR 450 it started at (the KX3 applies a per-mode filter on a mode change). `M CWR 0` while already in CWR leaves 450. Restore the filter on the radio or with `M CWR 450`.

Verified on the user's KX3 (2026-11-05, `rig-tui` from `main` at b06bdb7 for the whole flow and acc7941 for the prompt layout, no `rigctld` running at the start, `/dev/kx3` -> `ttyUSB0`, the user in `dialout`, a scratch config with one `[rig.kx3]` profile at `127.0.0.1:4532` and scratch cache and data dirs, `--offline`; the agent drove `rig-tui` in its own pty with a terminal emulator because `HERDR_ENV` was unset; read only, nothing transmitted, no CW, no tuning or mode change from the app):
- The start-`rigctld` prompt: about 9 s after start, with the strip showing "disconnected", it listed `/dev/serial/by-id/usb-FTDI_FT232R_USB_UART_AL05YPJL-if00-port0` for `kx3`. Enter started `rigctld -m 2045 -s 38400 -r <that path> -t 4532` as a child of `rig-tui`; the strip then showed `7.069.920 CWR 450 ... connected`, VFO A, split off, and raw `rigctld` over TCP read back the same (`f` 7069920, `m` CWR 450, `v` VFOA, `s` 0 VFOA, `i` 0, `x` empty).
- `Ctrl-D` while that daemon held the port gave "no serial ports found". `Ctrl-C` then `y` quit `rig-tui`, and `pgrep rigctld` showed no daemon afterwards. The two-line prompt layout (path, then rig and settings) was checked on the real by-id name at acc7941 with Esc and a quit; no daemon was started in that run.
- The radio was read before (a temporary `rigctld` started by the agent) and after: `f`, `m`, `v`, `s`, `i`, `x` were identical, so nothing needed restoring.
- Hamlib 4.7.2's `rigctld` does not exit when it cannot open its port (checked with a port name that does not exist): it prints `IO error` and keeps running, and its stderr is block-buffered when piped, so the text only appears when it exits. `rig-tui` therefore reports a daemon only when it exits.

Verified on the user's FT-847 (2026-11-04, `rig-tui` from `integration/spec-54` at af12631, rigctld on `127.0.0.1:4533` already running, no antenna connected, nothing transmitted, no PTT or CW; the agent drove `rig-tui` in tmux with a scratch config and a copy of the satellite cache, and restored the radio's state afterwards):
- `rig-tui --rig ft847`: the strip showed frequency, mode and passband, and split with its transmit frequency. `v` is not implemented by the backend (`RPRT -11`), so the strip shows "VFO not supported". Raw `rigctld` over TCP: `s` reads `1 Sub` with split on, `i` and `x` return the split transmit frequency and mode.
- The Satellite Activity: the pass list, live panel and transmitter list showed data from the cached TLEs and SatNOGS transmitters; transmitters with no uplink are marked receive-only.
- Tracking (`t`) ISS "Mode V/V FM (crew R2+3)" retuned the radio: receive 145.800000 MHz FM and transmit 144.490000 MHz FM, both read back through raw `rigctld` with split still on.
- Doppler during the ISS pass of 02:03:47Z to 02:08:51Z: sampled every 20 s through `rigctld`, receive moved from +2.4 kHz to about +0.3 kHz and transmit mirrored it (-2.38 kHz to -0.29 kHz) as the satellite neared closest approach. This shows the commanded frequencies only; nothing was received.
- With no antenna the strip read `S9+60`; that says nothing about the meter's scale.

Verified on the user's FT-847 (2026-11-05, `rig-tui` from `main` at 06b45c5, a temporary `rigctld` model 1001 on `127.0.0.1:4533` started by the agent, a dummy load on the HF port only and nothing on VHF/UHF, nothing transmitted except the HF keys below; the agent drove `rig-tui` in tmux with a scratch config and Log, and restored the radio's state afterwards: RX 144.300 MHz FM 9000, split off):
- The CAT baud is 9600: `f` answered at 9600 and timed out (`RPRT -5`) at 4800 and 57600.
- PTT through raw `rigctld` on HF into the dummy load (14.200 MHz USB, three keys of 2 s, one of 1 s): `T 1` returned `RPRT 0`, `t` read 1 during and 0 after, and the user heard the fan spool up for TX. During a key `l ALC` read 0, `l STRENGTH` 60 and `l RAWSTR` 31; on receive with the dummy load `STRENGTH` was -54 and `RAWSTR` 0. The user did not report the TX indicator or any RF output.
- SAT mode: hamlib's ft847 backend lists no SATMODE function. Split on turns it on (raw `S 1 Sub`, and tracking, which sends `S 1 VFOB`) and `S 0 VFOA` (what `Z` sends) turns it off; the user saw the SAT indicator follow each on the display. `S 0 Sub` returns `RPRT -11`.
- Tracking (`t`) RS-44 "Mode V/U - Transponder" before AOS: split went from `0 Sub` to `1 Sub`, receive 435.640000 MHz USB and transmit 145.965000 MHz LSB, matching the strip and live panel, read back through raw `rigctld`. The nudge keys `]` `[` `}` `{` (1 kHz and 100 Hz): about 2 s after a key the radio showed receive at nominal + n and transmit at nominal - n (+1900 Hz gave 435.641900 and 145.963100 MHz).
- The satellite Contact prefill: `l` while tracking, saved to a scratch Log: `FREQ` 145.965000 (`BAND` 2m), `MODE` SSB, `SUBMODE` LSB, `PROP_MODE` SAT, `SAT_NAME` "RS-44 & BREEZE-KM R/B", `SAT_MODE` V/U, `FREQ_RX` 435.640000 (`BAND_RX` 70cm).
- `Z` while tracking is broken (#81): the SAT indicator went off, then came back within seconds, because tracking kept running and read the radio's receive frequency falling back to 144.300 MHz as a dial turn (the panel showed `nudge -5794310000 Hz`).
- The status message "not a frequency:" comes from `f` then `Enter` on empty text. The earlier appearance was a stray keystroke, not a defect.
- Reconnect: with `rigctld` stopped the strip showed dashes and "disconnected" at once, and about 4 s after restarting it the strip showed "connected" and refilled by itself.
- Right after the `rigctld` restart `s` read `0 Sub` and `i` read 0 although the radio had last been in split; the user did not look at its display then. Hamlib's ft847 backend has no split read-back.
- Config dialog on a scratch config: editing the callsign and `Ctrl-S` changed only the `callsign` line; a change made outside the app to another part of the file was kept; a change outside to the same field showed "file W9EXT | yours N0CALLXYZ" and the file was not overwritten.

Verified on the uConsole's GPS (2026-11-05, HackerGadgets AIO v2 on a CM4, uncommitted work on `main` after 4108525, gpsd 3.25 with `DEVICES="/dev/ttyS0"` and `-n`, GPS powered by `gpio=27=op,dh`; `rig-tui --offline` driven in a Python pty with a scratch config, no rig connected):
- gpsd listed the u-blox on `/dev/ttyS0` at 9600 baud and reported a 3D fix (`TPV` mode 3). The strip showed the GPS Locator and coordinates with no Locator configured (`GPS EN71..  …N …W`), then `EN71..  …N …W` with one.
- `Ctrl-P` outside the dialog set the session Locator ("Locator EN71.. from GPS"); in the config dialog the Locator line read `GPS: EN71.. (3D fix); Ctrl-P fills it in`, `Ctrl-P` filled the field and `Ctrl-S` wrote only `[station] locator = "EN71.."`, no coordinates.

Still unverified (say "untested on hardware"):
- GPS: losing the fix or gpsd while `rig-tui` runs (`GPS no fix`, reconnect), and a 2D-only fix. The Satellite Activity does not use the GPS position.
- In the Satellite Activity: `Z` while tracking (#81 is open), and Doppler correction with a real pass on air. `sat-tui`'s UI-free code (tracker, cache, SatNOGS parsing) has unit tests only. Every other FT-847 behaviour not listed above is untested too.
- The S-meter with a real signal (the KX3's quiet-band value, and the FT-847's no-antenna `S9+60` and dummy-load -54, were the only ones seen), the TX indicator and RF output during PTT (only the fan was heard), and `rig-tui`'s display while the transmitter is keyed: PTT was keyed through raw `rigctld`, not the app.
- The start-`rigctld` prompt (see the KX3 entry of 2026-11-05) on anything but the KX3 with one port: the FT-847 (model 1001, 9600 baud), a second port, the Esc path against a real port, a port that cannot be opened, a missing `rigctld`, and a `rigctld` that exits by itself. Hamlib 4.6 (Raspberry Pi OS Trixie ships 4.6.x) and the ClockworkPi/CM4 are untested too.
- Sending CW through `rig-tui` (the CW panel, F1-F6 Soft keys, input line) and the `TB;` decode polling while the panel is active: `rigctld-client` sending and decoding is verified above, `rig-tui`'s use of it is not.
- The 2 Hz / 4 Hz polling cadences as such: the strip updated, but timing was not measured (a count of reads in `rigctld`'s `-vvvvv` log could not separate the app's reads from the test's own).
- The config dialog's read-only and parse-error paths and the discard prompt: `Esc` did not close the dialog after a conflict on the FT-847 setup, and that was not traced.
- Sending when the text exceeds the KX3's 24-char `KY` packet or its buffer (hamlib is expected to chunk it).
- Whether correcting both uplink and downlink is what the FT-847's sat mode needs on air: the sign of the correction is as expected, but only commanded frequencies were read back.

## Rules
- Commit and push only when asked.

## Agent skills

### Issue tracker

GitHub Issues on matthew-a-gordon/hamtools, via the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

Default five labels (`needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`). See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: one `GLOSSARY.md` and `docs/adr/` at the repo root. See `docs/agents/domain.md`.
