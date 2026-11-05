---
title: "Starting rigctld for the user: list ports without opening them, and do not try to detect a bad port from its output"
author: Matthew
date: 2026-11-05
branch: main
cost: { minutes: 9, tokens: 133601 }
id: l-d4559270
scope: repo
status: active
src: agent
touches:
  - crates/rig-tui/src/daemon.rs
  - crates/rig-tui/src/daemon_modal.rs
  - crates/rig-tui/src/app/daemon_offer.rs
---

## Decided
The app lists serial ports (directory entries only) and the user picks the rig profile per port; the app then starts rigctld as a child and kills it on quit. Model and baud come from the profile kind, because probing unknown devices could confuse a GPS dongle. This keeps the rule that only rigctld opens serial ports.

## Tried and ruled out
Detecting a bad port live from rigctld's stderr. With hamlib 4.7.2, an unopenable port prints "IO error" but rigctld keeps running with no rig, and its stderr is block-buffered when piped (stdbuf does not change it), so the text only arrives when it exits. Only exit is detected; the guide tells the user the strip will show rig errors instead.
Matching any line containing "error": a healthy start prints "rig_open: error = Requesting ...", so that would flag working daemons.

## Watch out
Not tried on a real radio or real serial port. Auto-offer is off unless main calls with_daemon_prompt(true), so existing connect-loop tests do not see the new ScanPorts command.

In the code: crates/rig-tui/src/daemon.rs (Daemons::poll, scan_ports), daemon_modal.rs, app/daemon_offer.rs, Ctrl-D chord in keys.rs.
