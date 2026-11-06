---
title: "Background services share one poll call, as a plain struct rather than a trait"
author: Matthew
date: 2026-11-06
branch: main
cost: { minutes: 5, tokens: 64284 }
id: l-073c6cb5
scope: repo
status: active
src: agent
touches:
  - crates/rig-tui/src/main.rs
---

## Decided
Geocode lookups, the gpsd watcher and satellite downloads are drained by one `poll` on a single struct, and that struct also takes the commands that belong to those services. The rig pool keeps its own drain because its events are tagged by rig.

## Tried and ruled out
A `Service` trait with one implementation per service. With three services and one caller it adds indirection and removes no code. Add the trait when a fourth service or a second caller appears.

## Watch out
Not run in the live TUI after the refactor: geocode lookup, GPS and satellite downloads were only checked by the build, clippy and the existing tests.

In the code: `Services` in crates/rig-tui/src/main.rs (`poll`, `handle`), used by `run` and `dispatch`.
