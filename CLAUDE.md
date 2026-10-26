# hamtools

Cargo workspace: `rigctld-client`, `kx3-cw`, `sat-tui`. Keep dependencies minimal; justify heavy crates.

## Commands
- `nix develop` (or direnv) for the toolchain and `hamlib_4`
- `rigctld -m 1 -t 4532` runs the dummy rig for tests
- `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
- `nix build` builds the packaged binaries (needs `Cargo.lock` committed)

## Boundaries
- Tools never open serial ports. Only `rigctld` does; everything else is a TCP client.
- KX3: `127.0.0.1:4532`, model 2045. FT-847: `127.0.0.1:4533`, model 1001 (not 1038). The daemons are managed by the user's NixOS config, not this repo.
- Observer position is runtime input (config, flags, later gpsd), never hard-coded. Defaults and fixtures use town-level Middlebury, IN only (~41.67 N, 85.71 W, EN71); no street address in the repo.

## Hardware caveats (no radio access here; say "untested on hardware" until the user confirms)
- KX3 CW decode via raw `TB;` through rigctld `w` is a hypothesis; the KX3 decoder must be enabled in its menu.
- KX3 USB baud assumed 38400; must match the radio's RS232 menu.
- FT-847 serial is 4800..57600, 8N2 in hamlib. Actual CAT baud is unknown (menu item 14).
- How hamlib's ft847 backend exposes sat mode/split/sub-VFO is unverified.

## Rules
- Commit and push only when asked.
