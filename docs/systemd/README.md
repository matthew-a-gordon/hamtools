# rigctld under systemd

Example udev rules and systemd units for running `rigctld` as a system
service per rig, instead of as a child of `rig-tui` (see "Starting
`rigctld`" in `docs/user-guide.md`). A `rigctld` managed this way survives
ssh disconnects and `rig-tui` restarts, and starts itself when the radio's
cable is plugged in. `rig-tui` needs no changes: it already retries the
connection every few seconds and reconnects when a daemon appears.

These are templates. `<serial>`, `<vendor>`, `<product>` and `<user>` are
placeholders — fill them in for your own cables and account before
installing.

Checked against Raspberry Pi OS Trixie's `libhamlib-utils` package
(hamlib 4.6.2): `rigctld --help` has `-T, --listen-addr`, and `rigctld -l`
lists both model 2045 (Elecraft KX3) and model 1001 (Yaesu FT-847,
*not* 1038) as Stable. *Untested on hardware* beyond that — the daemons
have not been run against either radio under hamlib 4.6.x.

## 1. Find each cable's USB attributes

Plug the cable in, find its tty, then walk up to the USB device:

```sh
ls -l /dev/serial/by-id/          # identifies the adapter by name, if it has one
udevadm info -a -n /dev/ttyUSB0   # or ttyACM0; repeat per adapter
```

In the output, the first "looking at parent device" block with
`ATTRS{idVendor}`, `ATTRS{idProduct}` and `ATTRS{serial}` is the one to
copy into `99-hamlib-rigs.rules`. Match on those, not on `ttyUSBn` —
that number is assigned in plug order and changes across reboots and
replugs.

The ClockworkPi uConsole's own built-in keyboard also shows up as a
serial port (`usb-ClockworkPI_uConsole_*`, typically `/dev/ttyACM0`). It
is not a radio; do not point a rule or a unit at it.

## 2. Edit the templates

- `99-hamlib-rigs.rules`: fill in `<serial>` for the KX3 line, and
  `<vendor>`/`<product>`/`<serial>` for the FT-847 line (its adapter may
  not be the same FTDI chip as the KX3's — check, don't assume).
- `rigctld-kx3.service` / `rigctld-ft847.service`: fill in `<user>` with
  the account that runs `rig-tui`. Confirm the `rigctld` path
  (`which rigctld`) matches `ExecStart` — it's `/usr/bin/rigctld` from
  the `libhamlib-utils` package on Debian and Raspberry Pi OS, but check
  on your system.

## 3. Install

Needs root. From this directory:

```sh
sudo cp 99-hamlib-rigs.rules /etc/udev/rules.d/
sudo cp rigctld-kx3.service rigctld-ft847.service /etc/systemd/system/
sudo udevadm control --reload
sudo udevadm trigger
sudo systemctl daemon-reload
sudo systemctl enable --now rigctld-kx3.service rigctld-ft847.service
```

`enable` wires up the `Wants` link regardless of whether the cable is
plugged in; the unit then starts by itself as soon as
`dev-kx3.device`/`dev-ft847.device` appears, and stops when the device
goes away. `--now` also tries to start it immediately — harmless either
way, but if the cable isn't plugged in yet that immediate start will
report a failure even though the unit is still correctly enabled for the
next hotplug. Install each rig's rule and unit once its cable is
plugged in and its udev attributes are known, rather than both at once
with one left as a placeholder.

## 4. Verify

```sh
systemctl status rigctld-kx3
journalctl -u rigctld-kx3 -b
```

With the cable plugged in, `rig-tui` should show the rig as connected
without ever showing the start-`rigctld` prompt — that prompt only
appears when `rig-tui` cannot reach a `rigctld` already running at the
profile's address.

A read-only raw check, cable plugged in and nothing else holding the
port:

```sh
printf 'f\n' | nc -N 127.0.0.1 4532   # KX3: get_freq
printf 'm\n' | nc -N 127.0.0.1 4532   # KX3: get_mode
```

(4533 for the FT-847.) `nc` needs `-N`, not `-q1`, to close after the
reply.
