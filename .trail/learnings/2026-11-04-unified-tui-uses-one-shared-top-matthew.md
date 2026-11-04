---
title: "Unified TUI uses one shared top strip, a floor of 80x24, and a fixed drop order for Activator screens"
author: Matthew
date: 2026-11-04
branch: main
cost: { minutes: 4, tokens: 59760 }
id: l-02fab001
scope: repo
status: active
src: agent
---

## Decided
- One shared top strip owned by the app (Activity, rig, frequency/mode, clock). Embedded sat-tui and rig-tui screens drop their own title, rig line and clock; sat-tui's footer keeps only TLE and SatNOGS ages. This saves 2 to 3 rows.
- On Activator screens short of rows, keep the rig strip, Send line, Soft key row and QSO count/reference. Drop the key hints first, then shrink CW decode, then spots.
- The floor is 80x24 for every Activity. 80x15 is not supported; DevTerm users pick a smaller console font.

## Tried and ruled out
- Keeping both headers as they are: costs rows at 80x15 and repeats the rig and clock.
- Merging only the duplicate text: the operator preferred a single strip.
- Supporting 80x15 for Satellite and General: the operator chose a single 80x24 floor for simpler tiers.

## Watch out
- Grids are computed from the display-size research, not measured (untested on hardware).
- What the app shows below 80x24 is not decided.

In the code: the render dumps came from `cargo run -p rig-tui --example prototype_launcher2 -- --dump 80x24` on branch `prototype/launcher-nav` (throwaway). The decision is recorded in GitHub issue #47 and the map, issue #34.
