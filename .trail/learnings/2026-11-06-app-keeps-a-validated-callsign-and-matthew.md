---
title: "App keeps a validated callsign and Locator, and split stays a struct; no VFO enum"
author: Matthew
date: 2026-11-06
branch: main
cost: { minutes: 9, tokens: 186354 }
id: l-2f456612
scope: repo
status: active
src: agent
touches:
  - crates/rig-tui/src/main.rs
  - crates/rig-tui/src/app.rs
  - crates/rig-tui/src/rigs.rs
---

## Decided
The app stores a validated callsign and Locator and drops an invalid configured one with a status-line note. Split is a small struct (enabled, transmit VFO). The Active rig's readings live in one place, its slot.

## Tried and ruled out
- A closed VFO enum: rigctld VFO names are open-ended (the FT-847 reports Sub; Main and currVFO exist), so it would need an Other(String) variant and gain little.
- A typed error on the HTTP transport: its one caller treats every error the same (back off, show the text), so a type would carry only a string.

## Watch out
A 4-character Locator in the config is now dropped at startup (Grid needs 6); it used to show in the strip and feed the observer position. Not run in the live TUI.

In the code: `with_callsign` and `with_locator` in crates/rig-tui/src/app.rs; `Split` in crates/rig-tui/src/rigs.rs.
