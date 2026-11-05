//! The "start rigctld" prompt: one row per serial port found, each with the rig profile to start
//! it for. Opened when the Active rig stays unreachable (once per session) or with `Ctrl-D`.
//! Holds nothing to lose; it only returns what to start, and `main` starts it.

use std::path::PathBuf;

use hamtools_config::RigKind;
use hamtools_tui::modal::Modal;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use crate::daemon::{Launch, Port, model_and_baud};
use crate::style::{dim, yellow};

/// A profile a daemon can be started for: a known kind on a loopback address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Target {
    /// Index into the app's rigs.
    pub rig: usize,
    pub name: String,
    pub kind: RigKind,
    pub tcp: u16,
}

pub(crate) enum Outcome {
    Stay,
    Close,
    Start(Launch),
}

pub(crate) struct DaemonModal {
    ports: Vec<Port>,
    targets: Vec<Target>,
    /// Per port, the index into `targets`.
    choice: Vec<usize>,
    selected: usize,
}

impl DaemonModal {
    /// `preferred`: the target index each port starts on (the Active rig's profile).
    pub(crate) fn new(ports: Vec<Port>, targets: Vec<Target>, preferred: usize) -> Self {
        let choice = vec![preferred.min(targets.len().saturating_sub(1)); ports.len()];
        Self {
            ports,
            targets,
            choice,
            selected: 0,
        }
    }

    pub(crate) fn on_key(&mut self, key: KeyEvent) -> Outcome {
        let n = self.ports.len();
        let t = self.targets.len();
        match key.code {
            KeyCode::Esc | KeyCode::Char('n' | 'q') => Outcome::Close,
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = (self.selected + n - 1) % n;
                Outcome::Stay
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected = (self.selected + 1) % n;
                Outcome::Stay
            }
            KeyCode::Left | KeyCode::BackTab => {
                self.choice[self.selected] = (self.choice[self.selected] + t - 1) % t;
                Outcome::Stay
            }
            KeyCode::Right | KeyCode::Tab => {
                self.choice[self.selected] = (self.choice[self.selected] + 1) % t;
                Outcome::Stay
            }
            KeyCode::Enter | KeyCode::Char('y') => {
                let target = &self.targets[self.choice[self.selected]];
                Outcome::Start(Launch {
                    rig: target.rig,
                    name: target.name.clone(),
                    kind: target.kind,
                    port: self.ports[self.selected].path.clone(),
                    tcp: target.tcp,
                })
            }
            _ => Outcome::Stay,
        }
    }

    /// Drop a port once a daemon holds it. `true` when none are left.
    pub(crate) fn remove(&mut self, port: &PathBuf) -> bool {
        if let Some(i) = self.ports.iter().position(|p| &p.path == port) {
            self.ports.remove(i);
            self.choice.remove(i);
            self.selected = self.selected.min(self.ports.len().saturating_sub(1));
        }
        self.ports.is_empty()
    }

    pub(crate) fn render(&self, frame: &mut Frame, area: Rect) {
        let rows = self.ports.len() as u16;
        let inner =
            Modal::new("rigctld is not running: start one?", 78, rows + 8).render(frame, area);
        let mut lines = vec![
            Line::styled("Serial ports found (listed, not opened):", yellow()),
            Line::raw(""),
        ];
        for (i, port) in self.ports.iter().enumerate() {
            let target = &self.targets[self.choice[i]];
            let (model, baud) = model_and_baud(target.kind).unwrap_or_default();
            let mark = if i == self.selected { ">" } else { " " };
            let text = format!(
                "{mark} {}  ->  {} (model {model}, {baud} baud, TCP {})",
                port.path.display(),
                target.name,
                target.tcp
            );
            lines.push(if i == self.selected {
                Line::raw(text)
            } else {
                Line::styled(text, dim())
            });
        }
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            "Up/Down port   Left/Right rig   Enter start   Esc not now",
            dim(),
        ));
        lines.push(Line::styled(
            "Needs the dialout group. The daemon stops when rig-tui quits.",
            dim(),
        ));
        frame.render_widget(Paragraph::new(lines), inner);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::KeyModifiers;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn modal() -> DaemonModal {
        let ports = vec![
            Port {
                path: "/dev/ttyUSB0".into(),
            },
            Port {
                path: "/dev/ttyUSB1".into(),
            },
        ];
        let targets = vec![
            Target {
                rig: 0,
                name: "kx3".into(),
                kind: RigKind::Kx3,
                tcp: 4532,
            },
            Target {
                rig: 1,
                name: "ft847".into(),
                kind: RigKind::Ft847,
                tcp: 4533,
            },
        ];
        DaemonModal::new(ports, targets, 0)
    }

    #[test]
    fn enter_starts_the_selected_port_for_the_chosen_rig() {
        let mut m = modal();
        m.on_key(key(KeyCode::Down));
        m.on_key(key(KeyCode::Right));
        let Outcome::Start(l) = m.on_key(key(KeyCode::Enter)) else {
            panic!("expected a launch");
        };
        assert_eq!(l.port, PathBuf::from("/dev/ttyUSB1"));
        assert_eq!((l.name.as_str(), l.tcp, l.rig), ("ft847", 4533, 1));
    }

    #[test]
    fn each_port_keeps_its_own_rig_choice() {
        let mut m = modal();
        m.on_key(key(KeyCode::Right));
        m.on_key(key(KeyCode::Down));
        let Outcome::Start(l) = m.on_key(key(KeyCode::Enter)) else {
            panic!("expected a launch");
        };
        assert_eq!(l.name, "kx3");
    }

    #[test]
    fn esc_declines_and_starts_nothing() {
        let mut m = modal();
        assert!(matches!(m.on_key(key(KeyCode::Esc)), Outcome::Close));
    }

    #[test]
    fn removing_the_last_port_empties_the_modal() {
        let mut m = modal();
        assert!(!m.remove(&PathBuf::from("/dev/ttyUSB0")));
        assert!(m.remove(&PathBuf::from("/dev/ttyUSB1")));
    }
}
