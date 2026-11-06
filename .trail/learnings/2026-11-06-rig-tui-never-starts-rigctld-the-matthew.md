---
title: "rig-tui never starts rigctld: the system runs one daemon per rig, and the app only reads and reconnects"
author: Matthew
date: 2026-11-06
branch: main
cost: { minutes: 34, tokens: 150973 }
id: l-b158970c
scope: repo
status: active
replaces: l-d4559270
src: agent
touches:
  - crates/rig-tui/src/app.rs
  - crates/rig-tui/src/ui.rs
  - crates/rig-tui/tests/rigs.rs
  - README.md
  - docs/user-guide.md
  - crates/rig-tui/src/lib.rs
  - crates/rig-tui/tests/app.rs
  - GLOSSARY.md
  - crates/rig-tui/Cargo.toml
  - crates/rig-tui/src/main.rs
---

## Decided
The on-demand launcher (prompt, Ctrl-D, serial-port scan, child process) is removed. Daemons belong to the system: systemd units bound to each radio's device by a udev rule on the ClockworkPi, the NixOS config on the author's machines. rig-tui stays a TCP client that reconnects by itself. When the Active rig is unreachable the status line keeps a yellow warning for as long as it is true, names a connected rig that is not Active, and Ctrl-R greys out rigs it cannot reach but still lets you pick them. The app never switches rigs on its own.

## Tried and ruled out
- Teaching the launcher to skip ports a system rigctld already holds, by reading other processes' command lines: it fixed one symptom of owning a second, weaker way to start daemons, so it was dropped with the launcher.
- Detecting a bad port from rigctld's output: hamlib 4.7.2's rigctld does not exit on a port it cannot open and block-buffers stderr when piped, so a unit cannot restart on that either; binding the unit to the device covers unplugging and replugging.
- Auto-switching to a connected rig at startup: a window opening on the wrong radio is a bad surprise, so the hint tells the operator and the startup rig is always rig.default or --rig.

## Watch out
The systemd and udev recipe is untested on hardware: not yet tried on the ClockworkPi, whose hamlib is probably 4.6.x.

In the code: docs/adr/0008-rig-tui-never-starts-rigctld.md, contrib/systemd/, the Colors and "Under systemd" sections of docs/user-guide.md.
