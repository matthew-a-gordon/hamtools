//! Offering to start `rigctld` for a serial port, on `App`. A child of `app` so it can reach
//! the rig slots. `App` stays pure: it asks `main` to list ports ([`Command::ScanPorts`]) and to
//! start a daemon ([`Command::StartDaemon`]); `main` answers through [`App::on_ports`].

use std::path::PathBuf;

use ratatui::Frame;
use ratatui::crossterm::event::KeyEvent;
use ratatui::layout::Rect;

use super::App;
use crate::daemon::{Port, local_tcp_port, model_and_baud};
use crate::daemon_modal::{DaemonModal, Outcome, Target};
use crate::rig::Command;

impl App {
    /// Offer the daemon prompt by itself when the Active rig stays unreachable (once per
    /// session). `main` turns this on; tests of the connect loop leave it off.
    pub fn with_daemon_prompt(mut self, on: bool) -> Self {
        self.daemon_auto = on;
        self
    }

    pub(super) fn daemon_open(&self) -> bool {
        self.daemon_modal.is_some()
    }

    /// The profiles a daemon can be started for: a known kind on a loopback address.
    fn daemon_targets(&self) -> Vec<Target> {
        if self.implicit_rig {
            return Vec::new();
        }
        self.rigs
            .iter()
            .enumerate()
            .filter(|(_, slot)| model_and_baud(slot.kind).is_some())
            .filter_map(|(rig, slot)| {
                Some(Target {
                    rig,
                    name: slot.name.clone(),
                    kind: slot.kind,
                    tcp: local_tcp_port(&slot.addr)?,
                })
            })
            .collect()
    }

    /// Ask `main` for the serial ports, because the operator pressed the Chord.
    pub(super) fn ask_for_ports(&mut self) -> Vec<Command> {
        self.daemon_manual = true;
        vec![Command::ScanPorts]
    }

    /// `main`'s answer to [`Command::ScanPorts`]: the ports no daemon of ours holds. Opens the
    /// prompt, or says why not when the operator asked.
    pub fn on_ports(&mut self, ports: Vec<Port>) {
        let manual = std::mem::take(&mut self.daemon_manual);
        if self.connected && !manual {
            return;
        }
        let targets = self.daemon_targets();
        let why = if targets.is_empty() {
            Some(
                "no local KX3 or FT-847 profile to start rigctld for (define [rig.NAME] with a \
                 127.0.0.1 address)",
            )
        } else if ports.is_empty() {
            Some("no serial ports found (looked for ttyUSB* and ttyACM*)")
        } else {
            None
        };
        if let Some(why) = why {
            if manual {
                self.status = Some(why.to_string());
            }
            return;
        }
        let preferred = targets
            .iter()
            .position(|t| t.rig == self.active)
            .unwrap_or(0);
        self.daemon_modal = Some(DaemonModal::new(ports, targets, preferred));
    }

    pub(super) fn on_daemon_key(&mut self, key: KeyEvent) -> Vec<Command> {
        let Some(modal) = &mut self.daemon_modal else {
            return Vec::new();
        };
        match modal.on_key(key) {
            Outcome::Stay => Vec::new(),
            Outcome::Close => {
                self.daemon_modal = None;
                Vec::new()
            }
            Outcome::Start(launch) => {
                let port: PathBuf = launch.port.clone();
                if modal.remove(&port) {
                    self.daemon_modal = None;
                }
                vec![Command::StartDaemon(launch)]
            }
        }
    }

    pub(super) fn render_daemon(&self, frame: &mut Frame, area: Rect) {
        if let Some(modal) = &self.daemon_modal {
            modal.render(frame, area);
        }
    }

    /// Whether the connect loop should ask for ports now: the rig has already been tried and is
    /// still down, nothing is on screen to be interrupted, and it has not been offered yet.
    pub(super) fn daemon_offer_due(&mut self) -> bool {
        let due = self.daemon_auto
            && !self.daemon_offered
            && self.last_connect.is_some()
            && !self.has_overlay();
        if due {
            self.daemon_offered = true;
        }
        due
    }
}
