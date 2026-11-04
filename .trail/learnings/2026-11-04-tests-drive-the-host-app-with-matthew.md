---
title: "Tests drive the host app, with two separate pure seams for the rate-limited client and the file formats"
author: Matthew
date: 2026-11-04
branch: main
cost: { minutes: 4, tokens: 48110 }
id: l-31b0f5ec
scope: repo
status: active
src: agent
touches:
  - GLOSSARY.md
  - crates/logbook/tests/golden.rs
---

## Decided
The unified TUI is tested at one main seam: the pure host App, driven with key events, rig events and ticks, reading back the commands it returns and the rendered buffer at fixed sizes. Two pure seams sit beside it: the reference client (fake clock and fake transport, so rate limits, backoff and caching need no network) and the file writers and embedded data (golden files and fixtures).

## Tried and ruled out
- Host App only: rate-limit and file-format bugs would only surface through screens.
- A tmux-driven end-to-end seam against a dummy rigctld: slow and fragile in CI, and it duplicates what App-level tests already cover.

## Watch out
Anything touching a real radio (the Satellite Activity, every FT-847 behaviour) still ends with an operator checklist and stays "untested on hardware".

In the code: existing App-level tests under crates/rig-tui/tests (app.rs, strip.rs, render.rs); golden-file prior art in crates/logbook/tests/golden.rs; dummy-rig tests in crates/rigctld-client/tests. Spec: GitHub issue 54.
