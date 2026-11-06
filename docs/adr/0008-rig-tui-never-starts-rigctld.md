# rig-tui never starts `rigctld`; the system runs one per rig

`rig-tui` used to offer to start a `rigctld` itself (a prompt when the Active rig stayed unreachable, `Ctrl-D` any time): it listed the serial ports, asked which profile each was, and ran `rigctld` as a child that stopped when the app quit. We removed it. Daemons are the system's job (systemd units on the ClockworkPi, the NixOS config on the author's machines), and `rig-tui` is only a TCP client of them, which keeps the boundary that only `rigctld` ever opens a serial port.

Why it went:

- With two rigs it is the wrong shape. A daemon owned by one app run serves one radio and dies with the app, including when an ssh session drops, while the point of two rigs is that `Ctrl-R` can switch between two always-ready daemons.
- It misbehaved next to the real setup. It only knew daemons it had started, so with a systemd unit already holding a port it offered that port again, pre-selected for the Active rig, and starting it would have opened a busy port. Teaching it to read other processes' command lines would have patched one symptom of owning a second, weaker way to do the same job.
- Its failure detection was weak. Hamlib 4.7.2's `rigctld` does not exit on a port it cannot open and block-buffers its stderr when piped, so a bad port could only be noticed when the daemon exited.
- It was only ever tried on one KX3 port.

What replaced it: `docs/systemd/` (a udev rule template that gives each radio a stable name and a systemd device tag, and one unit per rig bound to its device) with the recipe in the user guide, "Under systemd (the ClockworkPi)". `rig-tui` already reconnects on its own, so nothing else changed there. The status line explains an unreachable rig with a yellow `⚠` that stays while it is true, points at a connected rig that is not Active, and the `Ctrl-R` switcher greys out rigs it cannot reach but still lets you choose them. The app never switches rigs by itself.

Reopen this if a platform without a service manager needs the app to start a daemon; start from a design that does not own the child process.
