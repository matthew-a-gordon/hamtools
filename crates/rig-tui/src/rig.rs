//! The rig as seen by `App`: events in, commands out, and the executor that does the I/O.
//!
//! `App` never touches the network. It emits [`Command`]s; [`Rig::execute`] runs one against the
//! `rigctld` and returns the [`RigEvent`]s to feed back through `App::on_rig`.

use std::collections::BTreeSet;
use std::fmt;
use std::time::Duration;

use rigctld_client::Client;

/// The rig's mode as hamlib names it. Other names are kept verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RigMode {
    Lsb,
    Usb,
    Cw,
    Cwr,
    Am,
    Fm,
    Rtty,
    RttyR,
    Other(String),
}

impl RigMode {
    pub fn parse(name: &str) -> Self {
        match name {
            "LSB" => Self::Lsb,
            "USB" => Self::Usb,
            "CW" => Self::Cw,
            "CWR" => Self::Cwr,
            "AM" => Self::Am,
            "FM" => Self::Fm,
            "RTTY" => Self::Rtty,
            "RTTYR" => Self::RttyR,
            other => Self::Other(other.to_string()),
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Lsb => "LSB",
            Self::Usb => "USB",
            Self::Cw => "CW",
            Self::Cwr => "CWR",
            Self::Am => "AM",
            Self::Fm => "FM",
            Self::Rtty => "RTTY",
            Self::RttyR => "RTTYR",
            Self::Other(name) => name,
        }
    }
}

impl fmt::Display for RigMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A part of the rig that some backends do not support. A call that comes back "not available"
/// marks its feature unsupported until the next connection; the control is then disabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Feature {
    Vfo,
    Split,
    /// The split transmit frequency (`i`) specifically.
    SplitFreq,
    SMeter,
    Ptt,
}

impl Feature {
    pub fn label(self) -> &'static str {
        match self {
            Self::Vfo => "VFO",
            Self::Split => "split",
            Self::SplitFreq => "split transmit frequency",
            Self::SMeter => "S-meter",
            Self::Ptt => "PTT",
        }
    }
}

/// What `main` must do in response to a key or a tick.
///
/// Every `Set*` is followed, in the same call, by a read-back of the rig's state. None of them
/// can key the transmitter, and there is no command that sets PTT.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Quit,
    /// Try to (re)connect to `rigctld`.
    Connect,
    /// Read frequency, mode, VFO, split and PTT.
    PollState,
    /// Read the S-meter.
    PollSMeter,
    SetFreq(u64),
    /// `passband` 0 asks for the mode's normal width (untested on hardware).
    SetMode {
        mode: RigMode,
        passband: i64,
    },
    SetVfo(String),
    SetSplit {
        enabled: bool,
        tx_vfo: String,
    },
    SetSplitFreq(u64),
    /// Key this text as CW. The only command that can transmit.
    Send(String),
    /// A raw request a Mode panel asked for, e.g. `TB;`.
    Panel(String),
}

/// What the rig (or the attempt to reach it) reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RigEvent {
    Connected,
    Disconnected(String),
    Freq(u64),
    Mode {
        mode: RigMode,
        passband: i64,
    },
    /// Hamlib VFO name, e.g. `VFOA`.
    Vfo(String),
    Split {
        enabled: bool,
        tx_vfo: String,
    },
    /// What the rig reports as the split transmit frequency (see `Rig::read_tx_frequency`).
    SplitFreq(u64),
    Ptt(bool),
    /// S-meter in dB relative to S9.
    Strength(i32),
    /// The backend does not support this; the matching control is disabled.
    Unsupported(Feature),
    /// A set was refused or failed; the text is for the status line.
    SetFailed(String),
    SendFailed(String),
    PanelReply {
        request: String,
        reply: Result<String, String>,
    },
}

/// One `rigctld` reached over TCP, or none yet.
pub struct Rig {
    addr: String,
    client: Option<Client>,
    timeout: Duration,
    unsupported: BTreeSet<Feature>,
}

impl Rig {
    /// A stalled `rigctld` must not freeze the screen.
    pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(2);

    pub fn new(addr: impl Into<String>) -> Self {
        Self {
            addr: addr.into(),
            client: None,
            timeout: Self::DEFAULT_TIMEOUT,
            unsupported: BTreeSet::new(),
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Run one command. A transport failure drops the connection and reports `Disconnected`;
    /// the app then asks to `Connect` again.
    pub async fn execute(&mut self, command: Command) -> Vec<RigEvent> {
        match command {
            Command::Quit => Vec::new(),
            Command::Connect => self.connect().await,
            _ if self.client.is_none() => Vec::new(),
            Command::PollState => self.poll_state().await,
            Command::PollSMeter => self.poll_smeter().await,
            Command::SetFreq(hz) => {
                self.set("set frequency", None, async |c| c.set_freq(hz).await)
                    .await
            }
            Command::SetMode { mode, passband } => {
                let name = mode.name().to_string();
                self.set("set mode", None, async |c| {
                    c.set_mode(&name, passband).await
                })
                .await
            }
            Command::SetVfo(vfo) => {
                self.set("set VFO", Some(Feature::Vfo), async |c| {
                    c.set_vfo(&vfo).await
                })
                .await
            }
            Command::SetSplit { enabled, tx_vfo } => {
                self.set("set split", Some(Feature::Split), async |c| {
                    c.set_split(enabled, &tx_vfo).await
                })
                .await
            }
            Command::SetSplitFreq(hz) => {
                self.set(
                    "set split transmit frequency",
                    Some(Feature::SplitFreq),
                    async |c| c.set_split_freq(hz).await,
                )
                .await
            }
            Command::Send(text) => self.send(&text).await,
            Command::Panel(request) => self.panel(request).await,
        }
    }

    async fn connect(&mut self) -> Vec<RigEvent> {
        let attempt = tokio::time::timeout(self.timeout, Client::connect(&self.addr)).await;
        match attempt {
            Ok(Ok(client)) => {
                self.client = Some(client);
                self.unsupported.clear();
                vec![RigEvent::Connected]
            }
            Ok(Err(e)) => vec![RigEvent::Disconnected(format!(
                "cannot reach rigctld at {}: {e}",
                self.addr
            ))],
            Err(_) => vec![RigEvent::Disconnected(format!(
                "cannot reach rigctld at {}: timed out",
                self.addr
            ))],
        }
    }

    /// Run `op` on the client under the timeout. `Err(None)` means the transport failed and the
    /// connection was dropped; `Err(Some(_))` is the rig refusing the request.
    async fn call<T>(
        &mut self,
        op: impl AsyncFnOnce(&mut Client) -> rigctld_client::Result<T>,
    ) -> Result<T, Failure> {
        let client = self.client.as_mut().expect("checked by execute");
        let result = tokio::time::timeout(self.timeout, op(client)).await;
        match result {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(rigctld_client::Error::Rig(code))) if NOT_SUPPORTED.contains(&code) => {
                Err(Failure::Unsupported)
            }
            Ok(Err(e @ rigctld_client::Error::Rig(_))) => Err(Failure::Refused(e.to_string())),
            Ok(Err(e)) => {
                self.client = None;
                Err(Failure::Lost(e.to_string()))
            }
            Err(_) => {
                self.client = None;
                Err(Failure::Lost("rigctld timed out".to_string()))
            }
        }
    }

    /// A read for polling. `None` when the read gave nothing: the connection is gone (reported
    /// once in `events`), the rig refused, or `feature` is unsupported (reported once, then
    /// skipped until the next connection).
    async fn read<T>(
        &mut self,
        feature: Option<Feature>,
        events: &mut Vec<RigEvent>,
        op: impl AsyncFnOnce(&mut Client) -> rigctld_client::Result<T>,
    ) -> Option<T> {
        self.client.as_ref()?;
        if feature.is_some_and(|f| self.unsupported.contains(&f)) {
            return None;
        }
        match self.call(op).await {
            Ok(value) => Some(value),
            Err(Failure::Lost(e)) => {
                events.push(RigEvent::Disconnected(e));
                None
            }
            Err(Failure::Unsupported) => {
                if let Some(f) = feature {
                    self.unsupported.insert(f);
                    events.push(RigEvent::Unsupported(f));
                }
                None
            }
            Err(Failure::Refused(_)) => None,
        }
    }

    async fn poll_state(&mut self) -> Vec<RigEvent> {
        let mut events = Vec::new();
        if let Some(hz) = self
            .read(None, &mut events, async |c| c.get_freq().await)
            .await
        {
            events.push(RigEvent::Freq(hz));
        }
        if let Some((mode, passband)) = self
            .read(None, &mut events, async |c| c.get_mode().await)
            .await
        {
            events.push(RigEvent::Mode {
                mode: RigMode::parse(&mode),
                passband,
            });
        }
        if let Some(vfo) = self
            .read(Some(Feature::Vfo), &mut events, async |c| c.get_vfo().await)
            .await
        {
            events.push(RigEvent::Vfo(vfo));
        }
        if let Some((enabled, tx_vfo)) = self
            .read(Some(Feature::Split), &mut events, async |c| {
                c.get_split().await
            })
            .await
        {
            if enabled && let Some(hz) = self.read_tx_frequency(&tx_vfo, &mut events).await {
                events.push(RigEvent::SplitFreq(hz));
            }
            events.push(RigEvent::Split { enabled, tx_vfo });
        }
        if let Some(on) = self
            .read(Some(Feature::Ptt), &mut events, async |c| c.get_ptt().await)
            .await
        {
            events.push(RigEvent::Ptt(on));
        }
        events
    }

    /// The single place the split transmit frequency is read from the rig, and so the place to
    /// change if the radio needs a different method.
    ///
    /// Today this asks hamlib's split-frequency read (`i`). **Untested on hardware, and known
    /// suspect:** on the KX3 with split on and TX VFO B, a raw `i` returned VFO A's frequency
    /// (CLAUDE.md, "Still unverified"). Until that is settled against the radio, the value
    /// returned here is what the rig reported, not a confirmed transmit frequency. `_tx_vfo` is
    /// passed so an alternative (for example reading that VFO directly) needs no caller changes.
    async fn read_tx_frequency(
        &mut self,
        _tx_vfo: &str,
        events: &mut Vec<RigEvent>,
    ) -> Option<u64> {
        self.read(Some(Feature::SplitFreq), events, async |c| {
            c.get_split_freq().await
        })
        .await
    }

    async fn poll_smeter(&mut self) -> Vec<RigEvent> {
        let mut events = Vec::new();
        let level = self
            .read(Some(Feature::SMeter), &mut events, async |c| {
                c.get_level("STRENGTH").await
            })
            .await;
        if let Some(db) = level.and_then(|v| v.parse::<i32>().ok()) {
            events.push(RigEvent::Strength(db));
        }
        events
    }

    /// Run a set, then read the state back at once so the strip shows what the rig now reports.
    /// A refusal is reported for the status line instead.
    async fn set(
        &mut self,
        what: &str,
        feature: Option<Feature>,
        op: impl AsyncFnOnce(&mut Client) -> rigctld_client::Result<()>,
    ) -> Vec<RigEvent> {
        if self.client.is_none() {
            return Vec::new();
        }
        if let Some(f) = feature
            && self.unsupported.contains(&f)
        {
            return vec![RigEvent::SetFailed(format!("{} not supported", f.label()))];
        }
        match self.call(op).await {
            Ok(()) => self.poll_state().await,
            Err(Failure::Lost(e)) => vec![RigEvent::Disconnected(e)],
            Err(Failure::Unsupported) => {
                let mut events = Vec::new();
                if let Some(f) = feature {
                    self.unsupported.insert(f);
                    events.push(RigEvent::Unsupported(f));
                    events.push(RigEvent::SetFailed(format!("{} not supported", f.label())));
                } else {
                    events.push(RigEvent::SetFailed(format!("{what} failed: not supported")));
                }
                events
            }
            Err(Failure::Refused(e)) => vec![RigEvent::SetFailed(format!("{what} failed: {e}"))],
        }
    }

    /// Line breaks become spaces: rigctld commands are one line, so a raw newline would start a
    /// second command.
    async fn send(&mut self, text: &str) -> Vec<RigEvent> {
        let one_line = text
            .split(['\r', '\n'])
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        match self.call(async |c| c.send_morse(&one_line).await).await {
            Ok(()) => Vec::new(),
            Err(Failure::Refused(e)) => vec![RigEvent::SendFailed(format!("send failed: {e}"))],
            Err(Failure::Unsupported) => vec![RigEvent::SendFailed(
                "send failed: not supported by this rig".to_string(),
            )],
            Err(Failure::Lost(e)) => vec![RigEvent::Disconnected(e)],
        }
    }

    async fn panel(&mut self, request: String) -> Vec<RigEvent> {
        match self.call(async |c| c.send_cmd(&request).await).await {
            Ok(reply) => vec![RigEvent::PanelReply {
                request,
                reply: Ok(reply),
            }],
            Err(Failure::Refused(e)) => vec![RigEvent::PanelReply {
                request,
                reply: Err(e),
            }],
            Err(Failure::Unsupported) => vec![RigEvent::PanelReply {
                request,
                reply: Err("not supported by this rig".to_string()),
            }],
            Err(Failure::Lost(e)) => vec![RigEvent::Disconnected(e)],
        }
    }
}

/// Hamlib's `RIG_ENIMPL` (-4) and `RIG_ENAVAIL` (-11): the backend does not do this.
const NOT_SUPPORTED: [i32; 2] = [-4, -11];

enum Failure {
    Unsupported,
    Refused(String),
    Lost(String),
}
