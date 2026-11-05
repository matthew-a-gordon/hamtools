---
title: "Satellite never retunes the radio without a key press, and strip tuning stays live while tracking"
author: Matthew
date: 2026-11-05
branch: integration/spec-54
cost: { minutes: 1, tokens: 1316 }
id: l-76775379
scope: repo
status: active
src: agent
touches:
  - crates/rig-tui/src/bandplan.rs
  - crates/rig-tui/src/refdata.rs
  - docs/adr/0003-reference-data-is-offline-first-behind-one-client.md
  - docs/adr/0004-config-schema-break-named-rig-profiles.md
---

## Decided
- Returning to Satellite keeps the selection but does not resume Doppler on its own; `t` restarts tracking and restores the dial nudge. Auto-resume would retune the FT-847 with no key press, which beats the wish to pick up from the dial.
- Strip tuning keys are not blocked while Satellite tracks (the dial nudge is the workflow). Only band-plan tuning and Hunter `t` are refused, since they would fight the Doppler loop.
- The daily summit-list CSV download is allowed under the "no SOTA API" rule; it is a static file, not an API call.
- The positional rig address argument is going away in favour of `--rig NAME`.

## Tried and ruled out
- Blocking strip tuning during tracking (spec story 37 as first written): it contradicts retuning by hand during a pass (story 57). #54 was amended.

## Watch out
- Tickets #72-#75 carry the follow-up work; the thin HF rows in the ARRL band-plan data need real sources, not code.

In the code: crates/rig-tui/src/satellite/ (tuning_blocked, on_leave), crates/rig-tui/src/bandplan.rs, crates/rig-tui/src/refdata.rs, docs/adr/0003-reference-data-is-offline-first-behind-one-client.md, docs/adr/0004-config-schema-break-named-rig-profiles.md.
