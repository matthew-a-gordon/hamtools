# Running against a real radio

Read this before touching a radio. Results go in `CLAUDE.md` under "Hardware status", dated, with the setup. Anything not recorded there stays *untested on hardware*.

## Ask first, then stay inside the answer

Ask what the user allows. The usual answer is **read only and retuning**: no PTT, no CW, no transmit of any kind. Without an explicit yes, assume nothing is allowed beyond reads. Only `rigctld` opens a serial port; the app and your checks are TCP clients.

## Steps

1. **Record the radio's state** before changing anything, with raw `rigctld` commands over TCP:
   `printf 'f\n' | nc -N 127.0.0.1 4533` (FT-847; the KX3 is `4532`). Read `f` (frequency), `m` (mode and passband), `v` (VFO), `s` (split), `i` and `x` (split transmit frequency and mode). Write the values down.
2. **Use a scratch config and cache**, never the user's: set `XDG_CONFIG_HOME`, `XDG_CACHE_HOME` and `XDG_DATA_HOME` to directories in the scratchpad, pass `--config` to a scratch `config.toml` with the rig profile and a Locator, and copy `~/.cache/hamtools/` in when the Satellite Activity needs TLE and transmitter data.
3. **Drive the app in a terminal pane** (`herdr pane`, see `CLAUDE.md`), not by guessing keys. Read the screen after each step. To reach an Activity, type its name into the `Ctrl-G` palette and press `Enter`; arrow navigation lagged.
4. **Read back what the app commanded** with the raw `rigctld` reads from step 1. A strip showing a value is not proof the radio holds it.
5. **Restore the state** from step 1 with the matching sets (`F`, `M`, `V`, `S`, `I`, `X`), quit the app, and read everything again. The restore counts only when the second read matches the first.
6. **Record it** in `CLAUDE.md`: date, `rig-tui` commit, `rigctld` model and address, what was transmitted (nothing), and what was *not* checked.

## Rig notes

- **FT-847** (`127.0.0.1:4533`, model 1001): the daemon is the user's, already running. `v` returns `RPRT -11` (the strip shows "VFO not supported"). `s` returns `1 Sub` with split on. There is no read-back for SAT mode, so engagement cannot be confirmed from `rigctld`.
- **KX3** (`127.0.0.1:4532`, model 2045, 38400 baud): the user must be in `dialout`; use `sg dialout -c '...'` if the login predates the group. A mode change alters the passband (CWR 450 became CWR 1000); restore it with `M CWR 450`. A temporary `rigctld` for this is started by the agent and stopped afterwards.
- **No antenna:** the S-meter reads a large value (S9+60 on the FT-847) and says nothing about its calibration. Say so when recording it.
- **Doppler:** sample `f`, `i` over a pass every 20 s through `rigctld`. That shows commanded frequencies only, not reception.
