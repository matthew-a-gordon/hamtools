# One shared theme styles every screen and dialog

Colours, borders and the selected-row style used to be chosen at each call site (`Color::Cyan` here, `Modifier::REVERSED` there), and dialogs and panes drifted apart. They now come from `hamtools_tui::theme`, with `rig-tui`'s `style.rs` re-exporting the short names. A screen or dialog never names a colour of its own where the theme has a word for it. The look was adopted from termtastic.

The rules:

- Secondary text (labels, hints, placeholders, unfocused borders) is dark gray; errors and bad states are red; warnings and text sent on the air are yellow; good states are green.
- A pane (`theme::panel`) has rounded corners and a dark gray border, yellow with a yellow title when focused. A dialog (`theme::dialog`, drawn by `Modal`) has a thick yellow border and a bold title, so it reads as sitting above the panes without relying on color.
- The selected row of a list and the focused field's label of a form are black on yellow (`theme::selected`), not reverse video. The soft-key bar and the CW input line stay reverse video on purpose.
- Cyan marks a state value (the VFO letter), magenta is the Activity name, and a chip (`theme::chip`) carries the mode: blue, red while transmitting. A lamp (`theme::lamp`) is lit in its color or dim gray.
- Key hints show the key in bold and its label in gray (`theme::hint_line`: chunks separated by two spaces, the first word of a chunk is the key).
- Only the 16 ANSI colors, and every colored state keeps a non-color cue (see the Colors section of the user guide).

The strip is open top and bottom (a titled rule above and below two rows), and its second row ends in a live S-meter gauge, green to S9, yellow to S9+20, red above.

We left out termtastic's inner padding in panes and dialogs: it shrinks the inner width and would break the fixed-size dialog layouts and the 80x24 floor. Tests assert border glyphs and key text, so a theme change means updating them, as this one did (`tests/modal.rs`, `tests/navigation.rs`, `tests/sota_chaser.rs`).

Reopen this if a screen needs a color the theme has no word for; add the word to the theme rather than a literal at the call site.
