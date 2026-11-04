# hamtools

Cargo workspace: `rigctld-client`, `rig-tui`, `sat-tui`, `logbook`, `orbit`, `hamtools-config`, `hamtools-tui`. Keep dependencies minimal; justify heavy crates.

## Commands
- `nix develop` (or direnv) for the toolchain and `hamlib_4`
- `rigctld -m 1 -t 4532` runs the dummy rig for tests
- `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
- `nix build` builds the packaged binaries (needs `Cargo.lock` committed)

## Boundaries
- Tools never open serial ports. Only `rigctld` does; everything else is a TCP client.
- KX3: `127.0.0.1:4532`, model 2045. FT-847: `127.0.0.1:4533`, model 1001 (not 1038). The daemons are managed by the user's NixOS config, not this repo.
- Observer position is runtime input (config, flags, later gpsd), never hard-coded. Defaults and fixtures use town-level Middlebury, IN only (~41.67 N, 85.71 W, EN71); no street address in the repo.

## Hardware status (say "untested on hardware" for anything not listed as verified)
Verified on the user's KX3 (2026-10-26, hamlib 4.7.2, rigctld model 2045):
- 38400 baud works on `/dev/kx3`. The user must be in `dialout` (a login session started before joining the group lacks it; use `sg dialout -c '...'`).
- `w TB;` through rigctld returns the documented `TBtrrs;` reply (`Reply: TB006F C E ;\0RPRT 0`; `rigctld-client` handles the framing). The KX3's decoder emits `*` for undecodable characters, so weak or fading signals give garbage; that is the radio, not the client.
- `b <text>` (`send_morse`) keys CW on the KX3; the user confirmed it on the radio display. `TB;`'s transmit count `t` drains as it sends.
- Raw `rigctld` protocol on the KX3 (2026-11-04, hamlib 4.7.2, model 2045, `-s 38400`, a temporary daemon run by the agent), checked by hand over TCP, not through `rigctld-client`: `f`, `m` (`CWR` with passband `400`), `v` (`VFOA`), `s`, `t` (PTT, read only) and `l STRENGTH` (a signed dB value relative to S9, `-54` on a quiet band) all read back. `V VFOB`/`V VFOA`, `S 1 VFOB`/`S 0 VFOA`, `F` and `M` all returned `RPRT 0` and read back correctly, then were restored. Nothing was transmitted.
- Split TX frequency (2026-11-04, same setup as above, raw `rigctld` over TCP, nothing transmitted, state restored): `i` is correct. `S 1 VFOB` makes the KX3 copy VFO A's frequency into VFO B (raw `FB;` read 7071000 before, 7069920 after), which is why `i` once seemed to return A's frequency. With split on, setting B afterwards (`V VFOB`, `F`) makes `i` return it (7071000, matching `FB;`), while `f` keeps returning the receive frequency. `FT1;` reads back with split on, `FT0;` with it off. With split off, `i` returns 0 and `x` returns an empty mode.
- `rig-tui` against the KX3 (2026-11-04, `rig-tui` from `main` at 6ff6fc1, a temporary `rigctld` model 2045 on 4532 started by the agent, driven in tmux; nothing transmitted, no CW keys pressed, radio state restored afterwards; tests that wrote files used a scratch config and Log): the two-row strip (frequency, mode and passband, step, connection, VFO, split with transmit frequency, RX, S-meter, UTC clock); `v` (VFO A/B), `s` (split on/off), `S` (split transmit frequency: strip showed `split B 7.071.000` and `FB;` read 7071000), Left/Right tuning (1 kHz steps), `f` then `7070` Enter (read back 7.070.000), and `m`/`M` mode cycling (CWR, AM, FM and back, each read back); stopping `rigctld` while `rig-tui` ran gave dashes and "disconnected" with the error, and restarting it reconnected on its own (seen at the next look, 9 s after the restart); the log form (`l`, frequency, mode CW, RST 599/599, saved Contact written to the Log with `MY_GRIDSQUARE` from the Locator, count "1 logged", `L` session list); with split on the form's `FREQ` was the transmit frequency (7.071000); the config dialog (`c`) opened, with `Ctrl-S` closing it; address lookup (one live Nominatim request, "Middlebury, Indiana", town-level) returned candidates with 6-character Locators (EN71DQ for Middlebury, Elkhart County), and the address was not stored. The S-meter showed `S0` for the quiet band's `STRENGTH` of -54.
- Mode changes on the KX3 change the passband: `rig-tui`'s mode set sends passband 0, and after cycling CWR to AM to FM and back the radio read CWR 1000 instead of the CWR 450 it started at (the KX3 applies a per-mode filter on a mode change). `M CWR 0` while already in CWR leaves 450. Restore the filter on the radio or with `M CWR 450`.

Still unverified (say "untested on hardware"):
- The S-meter with a real signal (only the quiet-band value was seen), and PTT while transmitting, in `rig-tui`: not tested because both need the transmitter keyed, and what is on the antenna is not known to the agent.
- Sending CW through `rig-tui` (the CW panel, F1-F6 Soft keys, input line) and the `TB;` decode polling while the panel is active: `rigctld-client` sending and decoding is verified above, `rig-tui`'s use of it is not.
- The 2 Hz / 4 Hz polling cadences as such: the strip updated, but timing was not measured.
- The config dialog's edit-and-save, conflict, read-only and parse-error paths and the discard prompt: on the radio setup it was only opened, an address looked up and a Locator picked, and `Ctrl-S` closed it with the file unchanged (the Locator was the same value).
- Sending when the text exceeds the KX3's 24-char `KY` packet or its buffer (hamlib is expected to chunk it).
- FT-847: serial is 4800..57600, 8N2 in hamlib, but the actual CAT baud is unknown (menu item 14).
- How hamlib's ft847 backend exposes sat mode, split and the sub-VFO.
- Whether both uplink and downlink Doppler correction is right for the FT-847's sat mode.

## Rules
- Commit and push only when asked.

## Agent skills

### Issue tracker

GitHub Issues on matthew-a-gordon/hamtools, via the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

Default five labels (`needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`). See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: one `GLOSSARY.md` and `docs/adr/` at the repo root. See `docs/agents/domain.md`.
