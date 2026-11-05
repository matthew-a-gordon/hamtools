---
title: Never end an example or default CW message with SK; use idiomatic closings
author: Matthew
date: 2026-11-05
branch: main
cost: { minutes: 53, tokens: 237313 }
id: l-e7c07570
scope: repo
status: active
src: agent
touches:
  - config.example.toml
  - docs/user-guide.md
  - crates/rig-tui/src/export_modal.rs
  - crates/rig-tui/tests/export_flow.rs
  - crates/rig-tui/tests/rigs.rs
  - crates/rig-tui/src/rigs.rs
  - CLAUDE.md
  - crates/rig-tui/src/export.rs
  - crates/rig-tui/src/plans.rs
  - crates/rig-tui/src/refdata.rs
---

## Decided
- Example and default CW messages never use SK. To hams SK also means "silent key" (the operator has died), and the user does not want it in their macros. Close with "TU 73 DE {MYCALL} EE", matching the built-in default F3.
- Messages written into docs, example config and defaults should be idiomatic CW procedure ("CQ CQ CQ DE X X K", "TU 73 DE X EE"), not invented phrasings like "R R 73 TU".

## Tried and ruled out
- Ending the example F3 with SK, which is a valid end-of-contact prosign. Ruled out by the user.

## Watch out
- A grep for `\bSK\b` is the quick check when adding new sample messages.

In the code: config.example.toml and docs/user-guide.md (F3 under [macros]); built-in defaults in crates/rig-tui/src/softkeys.rs.
