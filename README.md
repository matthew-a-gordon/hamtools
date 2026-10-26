# hamtools

Rust TUIs (ratatui) for ham rig control and satellite work. Targets an Elecraft KX3 and a Yaesu FT-847, both driven through Hamlib's `rigctld`.

| Crate | Purpose |
| --- | --- |
| `rigctld-client` | Async client for the rigctld text protocol |
| `kx3-cw` | Decode and send CW on the KX3 over CAT |
| `sat-tui` | Pass prediction, Doppler-corrected rig tracking, SatNOGS transmitter data |

Status: scaffolding. Nothing here has been tested on real hardware yet.

## Development

```sh
nix develop            # or `direnv allow`
rigctld -m 1 -t 4532   # dummy rig for tests and development
cargo test --workspace
```

`nix build` produces `packages.<system>.default` (alias `hamtools`) containing all binaries.

## Architecture

`rigctld` owns each serial port; every tool here is a TCP client. KX3 is expected at `127.0.0.1:4532` (hamlib model 2045) and FT-847 at `127.0.0.1:4533` (model 1001).
