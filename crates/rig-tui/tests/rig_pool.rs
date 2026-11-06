//! The Rig pool: rig I/O off the screen's thread. Driven through `send`, `drain` and `settle`
//! against the scripted `rigctld` in `common`.

mod common;

use std::time::Duration;

use common::{RigState, start_on_thread};
use rig_tui::{Command, RigEvent, RigPool};

const SETTLE: Duration = Duration::from_secs(5);

#[test]
fn a_poll_reports_the_rigs_state_tagged_with_its_rig() {
    let fake = start_on_thread(RigState::default());
    let pool = RigPool::new([fake.addr.to_string()]);

    pool.send(0, Command::Connect);
    pool.send(0, Command::PollState);
    let events = pool.settle(SETTLE);

    assert!(events.contains(&(0, RigEvent::Connected)), "{events:?}");
    assert!(
        events.contains(&(0, RigEvent::Freq(14_074_000))),
        "{events:?}"
    );
}

fn count(fake: &common::FakeRigctld, line: &str) -> usize {
    fake.commands()
        .iter()
        .filter(|c| c.as_str() == line)
        .count()
}

fn slow(fake: &common::FakeRigctld, ms: u64) {
    fake.delay_ms.store(ms, std::sync::atomic::Ordering::SeqCst);
}

#[test]
fn commands_run_in_order_and_each_rig_is_tagged() {
    let (a, b) = (
        start_on_thread(RigState::default()),
        start_on_thread(RigState {
            freq: 7_030_000,
            ..RigState::default()
        }),
    );
    let pool = RigPool::new([a.addr.to_string(), b.addr.to_string()]);

    for rig in [0, 1] {
        pool.send(rig, Command::Connect);
    }
    pool.send(0, Command::SetFreq(14_200_000));
    pool.send(0, Command::PollState);
    pool.send(1, Command::PollState);
    let events = pool.settle(SETTLE);

    assert!(
        events.contains(&(0, RigEvent::Freq(14_200_000))),
        "{events:?}"
    );
    assert!(
        events.contains(&(1, RigEvent::Freq(7_030_000))),
        "{events:?}"
    );
    let sent = a.commands();
    let set = sent.iter().position(|c| c == "+F 14200000").unwrap();
    let read = sent.iter().rposition(|c| c == "+f").unwrap();
    assert!(set < read, "{sent:?}");
}

#[test]
fn repeated_polls_are_dropped_while_the_radio_is_busy() {
    let fake = start_on_thread(RigState::default());
    let pool = RigPool::new([fake.addr.to_string()]);
    pool.send(0, Command::Connect);
    pool.settle(SETTLE);
    slow(&fake, 150);

    for _ in 0..5 {
        pool.send(0, Command::PollState);
        pool.send(0, Command::PollSMeter);
    }
    pool.settle(SETTLE);

    assert_eq!(count(&fake, "+f"), 1, "{:?}", fake.commands());
    assert_eq!(count(&fake, "+l STRENGTH"), 1, "{:?}", fake.commands());
}

/// Keep the worker busy so what follows is queued behind it.
fn block(pool: &RigPool, fake: &common::FakeRigctld) {
    pool.send(0, Command::Connect);
    pool.settle(SETTLE);
    slow(fake, 100);
    pool.send(0, Command::SetFreq(14_100_000));
}

#[test]
fn an_identical_panel_request_already_queued_is_dropped() {
    let fake = start_on_thread(RigState::default());
    let pool = RigPool::new([fake.addr.to_string()]);
    block(&pool, &fake);

    for _ in 0..3 {
        pool.send(0, Command::Panel("TB;".to_string()));
    }
    pool.send(0, Command::Panel("OTHER;".to_string()));
    pool.settle(SETTLE);

    assert_eq!(count(&fake, "+w TB;"), 1, "{:?}", fake.commands());
    assert_eq!(count(&fake, "+w OTHER;"), 1, "{:?}", fake.commands());
}

#[test]
fn a_newer_doppler_frequency_replaces_a_queued_one_of_the_same_kind() {
    use sat_tui::tracker::RigCommand;
    let fake = start_on_thread(RigState::default());
    let pool = RigPool::new([fake.addr.to_string()]);
    block(&pool, &fake);

    pool.send(0, Command::Doppler(RigCommand::RxFreq(145_800_000)));
    pool.send(0, Command::Doppler(RigCommand::TxFreq(144_490_000)));
    pool.send(0, Command::Doppler(RigCommand::RxFreq(145_800_500)));
    pool.settle(SETTLE);

    let sent = fake.commands();
    assert_eq!(count(&fake, "+F 145800500"), 1, "{sent:?}");
    assert_eq!(count(&fake, "+F 145800000"), 0, "{sent:?}");
    assert_eq!(count(&fake, "+I 144490000"), 1, "{sent:?}");
    // The newer step runs after the other kind, not in the old one's place.
    let tx = sent.iter().position(|c| c == "+I 144490000").unwrap();
    let rx = sent.iter().position(|c| c == "+F 145800500").unwrap();
    assert!(tx < rx, "{sent:?}");
}

fn disconnected(events: &[(usize, RigEvent)]) -> bool {
    events
        .iter()
        .any(|(_, e)| matches!(e, RigEvent::Disconnected(_)))
}

#[test]
fn a_lost_connection_fails_the_commands_queued_behind_it_instead_of_sending_them() {
    let fake = start_on_thread(RigState::default());
    let pool = RigPool::new([fake.addr.to_string()]);
    pool.send(0, Command::Connect);
    pool.settle(SETTLE);
    slow(&fake, 100);
    fake.hang_up
        .store(true, std::sync::atomic::Ordering::SeqCst);

    pool.send(0, Command::PollState); // the radio hangs up on this one
    pool.send(0, Command::Send("CQ TEST".to_string()));
    pool.send(0, Command::SetFreq(7_030_000));
    let events = pool.settle(SETTLE);

    assert!(disconnected(&events), "{events:?}");
    assert!(
        events
            .iter()
            .any(|(_, e)| matches!(e, RigEvent::SendFailed(_))),
        "{events:?}"
    );
    assert!(
        events
            .iter()
            .any(|(_, e)| matches!(e, RigEvent::SetFailed(_))),
        "{events:?}"
    );
    let sent = fake.commands();
    assert_eq!(count(&fake, "+b CQ TEST"), 0, "{sent:?}");
    assert_eq!(count(&fake, "+F 7030000"), 0, "{sent:?}");
}

#[test]
fn cancelled_sends_never_key_the_radio() {
    let fake = start_on_thread(RigState::default());
    let pool = RigPool::new([fake.addr.to_string()]);
    block(&pool, &fake);

    pool.send(0, Command::Send("CQ TEST".to_string()));
    pool.send(0, Command::SetFreq(7_030_000));
    pool.cancel_sends();
    let events = pool.settle(SETTLE);

    assert_eq!(count(&fake, "+b CQ TEST"), 0, "{:?}", fake.commands());
    // Only transmitting is cancelled; a tuning change the operator asked for still happens.
    assert_eq!(count(&fake, "+F 7030000"), 1, "{:?}", fake.commands());
    assert!(
        events
            .iter()
            .any(|(_, e)| matches!(e, RigEvent::SendFailed(why) if why.contains("cancelled"))),
        "{events:?}"
    );
}

#[test]
fn a_new_address_discards_the_old_rigs_queue_and_reconnects_to_the_new_one() {
    let old = start_on_thread(RigState::default());
    let new = start_on_thread(RigState {
        freq: 7_074_000,
        ..RigState::default()
    });
    let pool = RigPool::new([old.addr.to_string()]);
    block(&pool, &old);

    pool.send(0, Command::SetFreq(7_030_000));
    pool.send(0, Command::Send("CQ TEST".to_string()));
    pool.send(
        0,
        Command::SetRigAddr {
            rig: 0,
            addr: new.addr.to_string(),
        },
    );
    pool.send(0, Command::Connect);
    pool.send(0, Command::PollState);
    let events = pool.settle(SETTLE);

    assert_eq!(count(&old, "+F 7030000"), 0, "{:?}", old.commands());
    assert_eq!(count(&old, "+b CQ TEST"), 0, "{:?}", old.commands());
    assert!(
        events.contains(&(0, RigEvent::Disconnected("address changed".to_string()))),
        "{events:?}"
    );
    assert!(
        events.contains(&(0, RigEvent::Freq(7_074_000))),
        "{events:?}"
    );
}

#[test]
fn a_stalled_idle_rig_does_not_delay_the_active_one() {
    let (active, idle) = (
        start_on_thread(RigState::default()),
        start_on_thread(RigState::default()),
    );
    let pool = RigPool::new([active.addr.to_string(), idle.addr.to_string()]);
    pool.send(0, Command::Connect);
    pool.send(1, Command::Connect);
    pool.settle(SETTLE);
    slow(&idle, 500);

    pool.send(1, Command::LightPoll(1)); // about a second on the idle rig
    pool.send(0, Command::PollState);
    let start = std::time::Instant::now();
    let mut seen = Vec::new();
    while start.elapsed() < Duration::from_millis(400)
        && !seen.contains(&(0, RigEvent::Freq(14_074_000)))
    {
        std::thread::sleep(Duration::from_millis(5));
        seen.extend(pool.drain());
    }

    assert!(seen.contains(&(0, RigEvent::Freq(14_074_000))), "{seen:?}");
    assert!(!seen.iter().any(|(rig, _)| *rig == 1), "{seen:?}");
}

#[test]
fn dropping_the_pool_does_not_wait_for_a_call_in_flight() {
    let fake = start_on_thread(RigState::default());
    let pool = RigPool::new([fake.addr.to_string()]);
    block(&pool, &fake);
    slow(&fake, 2_000);
    std::thread::sleep(Duration::from_millis(50)); // the call is now waiting on the radio

    let start = std::time::Instant::now();
    drop(pool);

    assert!(start.elapsed() < Duration::from_millis(500));
}
