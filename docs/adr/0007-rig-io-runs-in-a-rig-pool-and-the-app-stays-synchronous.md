# Rig I/O runs in a Rig pool on its own thread, and `App` stays synchronous

`dispatch` in `main.rs` runs every rig command with `rt.block_on(rig.execute(..))` inside the draw loop, so a slow or stalled `rigctld` (the client's timeout is 2 s) freezes the screen and the CW panel's polling, and a hung idle Rig delays the Active rig. The gpsd watcher, address lookup, downloads and daemons already run on threads behind channels; the rig was the odd one out.

We chose a **Rig pool** module: one worker task per Rig, all on one dedicated thread running a `current_thread` tokio runtime (features `rt`, `net`, `time`, plus `sync` for the queue's wake-up; no `rt-multi-thread`). Its interface is `new(addrs)`, `send(rig, Command)`, `drain()` returning `(rig, RigEvent)` pairs, `cancel_sends()`, and `settle(timeout)` for tests. The run loop sends and drains; it never waits on a radio. `Rig::execute` stays public and unchanged, so the existing protocol tests keep their seam.

Rules the pool keeps:

- Per Rig, commands run in order. A new `PollState`, `PollSMeter` or `LightPoll` is dropped if one of that kind is already queued or running. A `Panel(text)` is dropped if an identical one is queued (not running). A `Doppler(RxFreq)` or `Doppler(TxFreq)` replaces a queued step of the same variant. Everything else is strict FIFO.
- A queued `Send` never runs late. When a Rig's transport fails, its queued non-poll commands fail with an error event instead of running after a reconnect. `Quit` and a switch of the Active rig discard queued `Send`s, and dropping the pool aborts the workers rather than flushing.
- Every event carries its Rig index and the loop always calls `App::on_rig_at`, so a Rig switch with commands in flight cannot apply one radio's readings to another. The old `background` flag in `dispatch` goes away.
- `SetRigAddr` discards that Rig's queued non-poll commands, drops the connection and emits one `Disconnected("address changed")`; the existing reconnect loop does the rest.
- `TICK` stays 100 ms, so a change can show up to about 100 ms later than with `block_on`. `TICK` also sets the CW panel's polling rate, so it is not shortened while commands are in flight.

## Rejected: an async `App` and main loop

Replacing the poll loop with `crossterm::EventStream` and `tokio::select!`, with an async `App`, is how termtastic works, and it suits a radio that pushes packets at the app. `rigctld` is request and response and there is no push source here, so the only gain would be input latency below the 100 ms tick. The cost is `.await` through `App`, which has no I/O and is tested by key events, and a heavier tokio feature set against "keep dependencies minimal". Reopen this if a genuinely push-based source (a gpsd watch stream, a rigctld feed) is moved into the main loop.
