//! The Rig pool: every [`Rig`]'s `rigctld` I/O, off the screen's thread (ADR 0007).
//!
//! The run loop [`send`](RigPool::send)s a [`Command`] for a named rig and later
//! [`drain`](RigPool::drain)s the [`RigEvent`]s, each tagged with the rig it came from. One
//! worker task per rig, all on one dedicated thread, so a stalled idle rig cannot hold up the
//! Active one.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use sat_tui::tracker::RigCommand;
use tokio::sync::Notify;

use crate::rig::{Command, Rig, RigEvent};

struct Lane {
    state: Mutex<LaneState>,
    wake: Notify,
}

#[derive(Default)]
struct LaneState {
    queue: VecDeque<Command>,
    /// The command being executed now, kept for the poll rule.
    running: Option<Command>,
}

/// Polls are repeated by the app on a timer; a slow radio must not build a backlog of them.
fn is_poll(command: &Command) -> bool {
    matches!(
        command,
        Command::PollState | Command::PollSMeter | Command::LightPoll(_)
    )
}

impl LaneState {
    /// Add `command` to the queue, applying the coalescing rules. The queue grows by one, or
    /// stays the same length when the command is dropped as a repeat or replaces a waiting one.
    ///
    /// - A poll is dropped if one of its kind is waiting or running.
    /// - A `Panel` request is dropped if an identical one is waiting (one already running does
    ///   not count: its reply will hold new data).
    /// - A `Doppler` frequency step replaces a waiting one of the same kind, and goes to the
    ///   back so it cannot jump ahead of commands the operator queued meanwhile.
    /// - Everything else is strict FIFO.
    fn push(&mut self, command: Command) {
        let kind = |c: &Command| std::mem::discriminant(c);
        if is_poll(&command) {
            let same = |other: &Command| kind(other) == kind(&command);
            if self.running.as_ref().is_some_and(same) || self.queue.iter().any(same) {
                return;
            }
        } else if let Command::Panel(request) = &command {
            if self
                .queue
                .iter()
                .any(|c| matches!(c, Command::Panel(other) if other == request))
            {
                return;
            }
        } else if let Command::Doppler(step) = &command
            && let Some(slot) = doppler_slot(step)
        {
            self.queue.retain(
                |c| !matches!(c, Command::Doppler(other) if doppler_slot(other) == Some(slot)),
            );
        }
        self.queue.push_back(command);
    }
}

impl LaneState {
    /// Remove the waiting commands that change the radio (sets and sends), returning the failure
    /// events to report for them. Polls, Doppler steps, panel requests and connection commands
    /// stay: they are repeated by the app or are about the connection itself.
    fn fail_changes(&mut self, why: &str) -> Vec<RigEvent> {
        let mut events = Vec::new();
        self.queue.retain(|command| match command {
            Command::Send(_) => {
                events.push(RigEvent::SendFailed(format!("not sent: {why}")));
                false
            }
            Command::SetFreq(_)
            | Command::SetMode { .. }
            | Command::SetVfo(_)
            | Command::SetSplit { .. }
            | Command::SetSplitFreq(_) => {
                events.push(RigEvent::SetFailed(format!("not applied: {why}")));
                false
            }
            _ => true,
        });
        events
    }
}

/// Which frequency a Doppler step sets, if it is a frequency step.
fn doppler_slot(step: &RigCommand) -> Option<bool> {
    match step {
        RigCommand::RxFreq(_) => Some(true),
        RigCommand::TxFreq(_) => Some(false),
        _ => None,
    }
}

pub struct RigPool {
    lanes: Vec<Arc<Lane>>,
    events: Receiver<(usize, RigEvent)>,
    reporter: Sender<(usize, RigEvent)>,
    pending: Arc<AtomicUsize>,
    stop: Arc<Notify>,
    thread: Option<JoinHandle<()>>,
}

impl RigPool {
    /// One worker per address, in rig order.
    pub fn new(addrs: impl IntoIterator<Item = String>) -> Self {
        Self::with_timeout(addrs, Rig::DEFAULT_TIMEOUT)
    }

    pub fn with_timeout(addrs: impl IntoIterator<Item = String>, timeout: Duration) -> Self {
        let addrs: Vec<String> = addrs.into_iter().collect();
        let lanes: Vec<Arc<Lane>> = addrs
            .iter()
            .map(|_| {
                Arc::new(Lane {
                    state: Mutex::new(LaneState::default()),
                    wake: Notify::new(),
                })
            })
            .collect();
        let (tx, events) = channel();
        let reporter = tx.clone();
        let pending = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(Notify::new());
        let thread = {
            let (lanes, pending, stop) = (lanes.clone(), Arc::clone(&pending), Arc::clone(&stop));
            std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("tokio runtime");
                let local = tokio::task::LocalSet::new();
                rt.block_on(local.run_until(async move {
                    for (i, addr) in addrs.into_iter().enumerate() {
                        let rig = Rig::new(addr).with_timeout(timeout);
                        tokio::task::spawn_local(work(
                            i,
                            rig,
                            Arc::clone(&lanes[i]),
                            tx.clone(),
                            Arc::clone(&pending),
                        ));
                    }
                    stop.notified().await;
                }));
            })
        };
        Self {
            lanes,
            events,
            reporter,
            pending,
            stop,
            thread: Some(thread),
        }
    }

    /// Queue `command` for rig `rig`. Commands that are not rig I/O are the caller's to run.
    pub fn send(&self, rig: usize, command: Command) {
        let Some(lane) = self.lanes.get(rig) else {
            return;
        };
        {
            let mut state = lane.state.lock().expect("lane");
            if matches!(command, Command::SetRigAddr { .. }) {
                // Whatever was queued was meant for the old address.
                self.pending.fetch_sub(state.queue.len(), Ordering::SeqCst);
                state.queue.clear();
            }
            let before = state.queue.len();
            state.push(command);
            if state.queue.len() > before {
                self.pending.fetch_add(1, Ordering::SeqCst);
            }
        }
        lane.wake.notify_one();
    }

    /// Drop every waiting `Send` on every rig (the Active rig changed, or the app is quitting) so
    /// nothing keys the transmitter late. One already running is not interrupted.
    pub fn cancel_sends(&self) {
        let mut events = Vec::new();
        for (index, lane) in self.lanes.iter().enumerate() {
            let mut state = lane.state.lock().expect("lane");
            let before = state.queue.len();
            state.queue.retain(|c| {
                let send = matches!(c, Command::Send(_));
                if send {
                    events.push((index, RigEvent::SendFailed("send cancelled".to_string())));
                }
                !send
            });
            self.pending
                .fetch_sub(before - state.queue.len(), Ordering::SeqCst);
        }
        for event in events {
            let _ = self.reporter.send(event);
        }
    }

    /// The events produced since the last call, in the order each rig produced them.
    pub fn drain(&self) -> Vec<(usize, RigEvent)> {
        self.events.try_iter().collect()
    }

    /// Wait until every queue is empty and no call is running (or `timeout`), then drain.
    pub fn settle(&self, timeout: Duration) -> Vec<(usize, RigEvent)> {
        let deadline = Instant::now() + timeout;
        while self.pending.load(Ordering::SeqCst) > 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        self.drain()
    }
}

impl Drop for RigPool {
    fn drop(&mut self) {
        self.stop.notify_one();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

async fn work(
    index: usize,
    mut rig: Rig,
    lane: Arc<Lane>,
    tx: Sender<(usize, RigEvent)>,
    pending: Arc<AtomicUsize>,
) {
    loop {
        let next = {
            let mut state = lane.state.lock().expect("lane");
            let next = state.queue.pop_front();
            state.running = next.clone();
            next
        };
        let Some(command) = next else {
            lane.wake.notified().await;
            continue;
        };
        let moved = matches!(command, Command::SetRigAddr { .. });
        let mut events = rig.execute(command).await;
        if moved {
            events.push(RigEvent::Disconnected("address changed".to_string()));
        }
        let dropped = events
            .iter()
            .any(|e| matches!(e, RigEvent::Disconnected(_)));
        let mut state = lane.state.lock().expect("lane");
        state.running = None;
        if dropped {
            // Whatever was queued for the old connection must not run on a new one.
            let before = state.queue.len();
            events.extend(state.fail_changes("connection lost"));
            pending.fetch_sub(before - state.queue.len(), Ordering::SeqCst);
        }
        drop(state);
        for event in events {
            let _ = tx.send((index, event));
        }
        pending.fetch_sub(1, Ordering::SeqCst);
    }
}
