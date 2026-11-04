# hamtools

Cargo workspace: `rigctld-client`, `kx3-cw`, `sat-tui`, `logbook`, `orbit`. Keep dependencies minimal; justify heavy crates.

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

Still unverified:
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
