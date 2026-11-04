# Terminal sizes of the rig-tui targets (research for #37, map #34)

Question: what columns x rows does a terminal give on ClockworkPi uConsole / DevTerm screens at sensible console font sizes, and what are typical laptop terminal sizes? Output: a table to design size tiers against.

## Sourced facts

- DevTerm display: 6.8 in IPS, 1280x480 (16:6). Source: https://www.clockworkpi.com/devterm
- uConsole display: 5.0 in IPS, **1280x720** (not 480; the map's "1280x480 class" fits the DevTerm only). Source: https://www.clockworkpi.com/uconsole (physical panel width/height not published there).
- Linux kernel console fonts (cell size in px): 8x16 (VGA; the FONT_AUTOSELECT fallback), 10x18 Terminus ("for 13-16 in laptops"), 12x22 (Sun), 16x32 Terminus ("for HiDPI screens", must be chosen manually), 6x10/6x11/7x14/8x8 smaller. Source: https://github.com/torvalds/linux/blob/master/lib/fonts/Kconfig
- Classic VGA text modes are 80x25 (also 80x50, 80x43, 80x30, 80x60). Source: https://docs.kernel.org/admin-guide/svga.html

## Derived console grids (computed, not measured)

cols = floor(width_px / cell_w), rows = floor(height_px / cell_h). Assumes a raw Linux console (fbcon) filling the panel, no rotation, no borders. A GUI terminal emulator on the same panel loses a little to window chrome, padding and any status bar, so treat these as upper bounds.

| Screen | 8x16 | 10x18 | 12x22 | 16x32 |
|---|---|---|---|---|
| DevTerm 1280x480 | 160x30 | 128x26 | 106x21 | 80x15 |
| uConsole 1280x720 | 160x45 | 128x40 | 106x32 | 80x22 |
| Laptop 1366x768 | 170x48 | 136x42 | 113x34 | 85x24 |
| Laptop 1920x1080 | 240x67 | 192x60 | 160x49 | 120x33 |

Laptop GUI terminals use proportionally different fonts and have chrome, so real full-screen values are a few rows/cols smaller than this table; HiDPI laptops (2560x1600 and up) are typically scaled so the result lands near the 1920x1080 row.

## Realistic design bands (proposal, derived)

Height is the scarcer axis on the small targets; width is generous everywhere except big fonts.

| Band | Representative grid | Where it comes from |
|---|---|---|
| Floor | 80x24 | Classic default terminal and VGA text mode; also DevTerm at 16x32 (80x15 is shorter, see note) and a default-size laptop window (the 80x24 default is convention, not confirmed from a primary source here) |
| Small | ~128x26 | DevTerm at 10x18 |
| Medium | ~160x30 to 160x45 | DevTerm / uConsole at 8x16 |
| Large | ~190-240 x 50-67 | 1080p laptop full-screen |

Notes for tiering:
- The DevTerm is the binding constraint on rows: 15 to 30 rows depending on font. A tier on rows <= ~26 is essentially "DevTerm"; on columns it is not constrained at 8x16 or 10x18.
- At 16x32 a 1280-wide screen is 80 cols, so a floor of 80 columns still holds; rows can fall to 15 (DevTerm) or 22 (uConsole).
- Suggested breakpoints to test prototypes against: 80x24, 80x15, 128x26, 160x30, 160x45, 240x67.

## Only the operator can measure

- The size of their current working terminal window (`tput cols; tput lines`, or `stty size`): defines the "smallest laptop case" in the map.
- The actual grid on their own ClockworkPi (console font in use, rotation, any window manager bars): `stty size` in a real tty. The uConsole panel is natively portrait, so rotation/fbcon setup could change the effective grid. Untested on hardware.
- Their full-screen laptop terminal (`stty size` fullscreen) and font choice.
