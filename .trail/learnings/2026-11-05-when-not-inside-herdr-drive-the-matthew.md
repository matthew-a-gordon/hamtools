---
title: "When not inside Herdr, drive the TUI in our own pty with a terminal emulator instead of controlling the user's Herdr"
author: Matthew
date: 2026-11-05
branch: main
cost: { minutes: 3, tokens: 23789 }
id: l-c6849cd3
scope: repo
status: active
src: agent
touches:
  - ".idea/.gitignore"
  - crates/rig-tui/src/app.rs
  - crates/rig-tui/src/app/daemon_offer.rs
  - crates/rig-tui/src/daemon.rs
  - crates/rig-tui/src/daemon_modal.rs
  - crates/rig-tui/src/keys.rs
  - crates/rig-tui/src/lib.rs
  - crates/rig-tui/src/main.rs
  - crates/rig-tui/src/rig.rs
  - crates/rig-tui/tests/daemon_prompt.rs
---

## Decided
If HERDR_ENV is not set, do not use `herdr pane` against the user's session. Run rig-tui in a pty from a small Python script, feed its output to pyte to get a readable screen, send keys, and print the rows that matter. Use a scratch config, cache and data dir, and `--offline`.

## Tried and ruled out
pyte is not installed on the host, and `nix shell nixpkgs#python3Packages.pyte` does not put it on the interpreter's path. What works: `nix shell --impure --expr 'with import <nixpkgs> {}; python3.withPackages (p: [p.pyte])' --command python3 script.py`.
Letting the script end without quitting the app: the closed pty hangs the app up and the child daemon dies too, which hides whether the real quit path (`Ctrl-C`, then `y`) kills it. Quit with `y` and check `pgrep rigctld` afterwards.

## Watch out
Take the radio's state with a temporary rigctld before and after (reads `f m v s i x`) and diff the two; the app's own daemon holds the port while the app runs. Only the by-id port path is long enough to truncate the start prompt's row at 78 columns (open bug, rig details cut off).

In the code: the procedure is in docs/agents/hardware-session.md; the prompt is crates/rig-tui/src/daemon_modal.rs.
