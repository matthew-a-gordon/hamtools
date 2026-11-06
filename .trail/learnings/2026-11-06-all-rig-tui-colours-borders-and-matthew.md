---
title: "All rig-tui colours, borders and selection styling come from one shared theme, adopted from termtastic"
author: Matthew
date: 2026-11-06
branch: main
cost: { minutes: 24, tokens: 169897 }
id: l-ae2fd06e
scope: repo
status: active
src: agent
touches:
  - crates/rig-tui/examples/prototype_termtastic.rs
  - crates/hamtools-tui/src/modal.rs
  - crates/rig-tui/src/style.rs
  - crates/rig-tui/src/ui.rs
---

## Decided
The look (colours, pane and dialog borders, selected-row style, chips, key hints) lives in one shared theme that every screen and dialog uses. Panes are rounded and dark gray, yellow when focused. Dialogs are thick and yellow. The selected row is black on yellow. Green, yellow and red mean good, warn and bad. Magenta is the app name only. The strip is open top and bottom, with a live S-meter gauge on its second row.

## Tried and ruled out
- Padding inside panes and dialogs (termtastic does this): it shrinks the inner width and would break the fixed-size dialog layouts and the 80x24 floor.
- A non-breaking space between a hint's key and label: it broke tests that look for text like "Ctrl-G go to".
- Changing the strip title separator from `|` to a box-drawing bar: tests and docs match the `|`.

## Watch out
Tests hardcode border glyphs and the old reverse-video selection, so a theme change means updating them. Selected overdue plans keep their red text on the yellow highlight. The soft-key bar and the CW input bar stay reverse-video on purpose.

In the code:
- crates/hamtools-tui/src/theme.rs holds the styles, `panel()` and `dialog()`; crates/hamtools-tui/src/modal.rs draws every dialog through it.
- crates/rig-tui/src/style.rs re-exports it; the strip is in crates/rig-tui/src/ui.rs.
