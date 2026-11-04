# The shared config drops the single rig address for named rig profiles, with no backward compatibility

Both radios are now described as named profiles (`[rig.kx3]`, `[rig.ft847]`, each with an address and a kind), with a `rig.default`. The old `[rig] addr` and `[kx3]` keys, `sat-tui.toml`, and sat-tui's `rig_addr` and `[observer]` are removed rather than read as aliases. We chose a clean break over compatibility because nothing is deployed and the config crate then has one schema to read and write. A static capability table keyed by profile kind replaces probing `rigctld`, since hamlib's claims and reality differ; rows not verified on hardware are labelled "untested on hardware".

Specified in #54; decided in "Rig switching and dual connections" (#41) and "Folding sat-tui into the Satellite Activity" (#42).
