# An Activity is an I/O-free trait object hosted above the Mode panel

The unified `rig-tui` hosts one Activity at a time (General, Satellite, SOTA, POTA). An Activity takes key events, ticks and rig replies and returns commands from a closed set that `main` performs; it does no I/O itself, and it may embed Mode panels. We chose this over folding each program into one big `App` match (every Activity would touch every module) and over a plugin system or a crate per Activity (machinery with a single consumer). Activities are registered statically in the `rig-tui` crate. Only Satellite may tune on its own (Doppler), and nothing but the existing CW send may key the transmitter; the closed command set is what makes that checkable.

Specified in #54; decided in "Activity model: what an Activity is and how the App hosts it" (#35).
