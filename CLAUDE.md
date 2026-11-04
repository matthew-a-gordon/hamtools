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

Still unverified:
- With split on and TX VFO B, `i` (split TX frequency) returned VFO A's frequency, not VFO B's; the cause is unknown. With split off, `i` returns 0 and `x` returns an empty mode. **Still open, untested on hardware through `rig-tui`.** `rig-tui` reads `i` in one place (`Rig::read_tx_frequency`) and every consumer takes the transmit frequency from `App::transmit_frequency()` (split off: the receive frequency; split on: the value `i` reported, or none, never the receive frequency). To close it, with the KX3 connected: set split on with TX VFO B, give VFO B a different frequency from VFO A, and compare `i` with what the radio displays for B; if `i` is wrong, change only `read_tx_frequency` (candidates: `\get_vfo`/`\set_vfo` to read VFO B directly and restore, or the Elecraft `FB;` raw command).
- The VFO, split, PTT and S-meter polling and the `v`, `s`, `S` keys in `rig-tui` run only against a scripted fake `rigctld`. The mode set uses passband 0, whose effect on the KX3 is unchecked.
- The 2 Hz / 4 Hz polling cadences, and the above through `rigctld-client` (the client has the calls; they have not been run against the KX3).
- `rig-tui`'s log form, session list, config dialog, address lookup (Nominatim), strip polling cadences and VFO/split keys: untested on hardware, run only against a scripted fake `rigctld` (and, for the lookup, a local fake server).
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
