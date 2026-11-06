---
title: "Testing the rig pool: the fake rigctld must run on its own thread, and slowness is simulated with a reply delay"
author: Matthew
date: 2026-11-06
branch: rig-pool
cost: { minutes: 26, tokens: 140307 }
id: l-55449840
scope: repo
status: active
src: agent
touches:
  - crates/hamtools-tui/src/form.rs
  - crates/hamtools-tui/src/lib.rs
  - crates/hamtools-tui/src/list.rs
  - crates/hamtools-tui/src/modal.rs
  - crates/hamtools-tui/src/theme.rs
  - crates/hamtools-tui/tests/modal.rs
  - crates/rig-tui/src/activator.rs
  - crates/rig-tui/src/bandplan.rs
  - crates/rig-tui/src/config_dialog.rs
  - crates/rig-tui/src/cw.rs
---

## Decided
Tests of the rig pool block their own thread while waiting for the worker to settle, so the scripted `rigctld` has to run on a runtime of its own thread, not on the test's runtime. A slow or stalled radio is simulated with a per-reply delay on the fake, which makes "the worker is busy, so what I send next is queued" deterministic without sleeps in the code under test.

## Tried and ruled out
- `#[tokio::test(flavor = "multi_thread")]` so the fake and the pool share a runtime: the dev-dependencies have no `rt-multi-thread` feature, and adding it only for tests was not worth it.
- Asserting `STRENGTH` twice after a state poll plus an S-meter poll: a state poll does not read `STRENGTH`; only the S-meter poll does. Count that line once.

## Watch out
Tests that check a command was dropped or replaced must first keep the worker busy (a slow `SetFreq` as a blocker) so the commands under test are still queued, not running. A "stalled idle rig" test measures wall-clock time (400 ms window against a 500 ms-per-reply fake), so a very loaded machine could make it flaky.

In the code:
- crates/rig-tui/tests/common/mod.rs: `start_on_thread`, `FakeRigctld::delay_ms`.
- crates/rig-tui/tests/rig_pool.rs: `block()` helper and the pool tests.
