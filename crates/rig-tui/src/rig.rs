//! The rig as seen by `App`: events in, commands out, and the executor that does the I/O.
//!
//! `App` never touches the network. It emits [`Command`]s; [`Rig::execute`] runs one against the
//! `rigctld` and returns the [`RigEvent`]s to feed back through `App::on_rig`.

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

/// What `main` must do in response to a key or a tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Quit,
    /// Try to (re)connect to `rigctld`.
    Connect,
    /// Read frequency and mode.
    PollState,
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
}

impl Rig {
    /// A stalled `rigctld` must not freeze the screen.
    pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(2);

    pub fn new(addr: impl Into<String>) -> Self {
        Self {
            addr: addr.into(),
            client: None,
            timeout: Self::DEFAULT_TIMEOUT,
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
            Command::Send(text) => self.send(&text).await,
            Command::Panel(request) => self.panel(request).await,
        }
    }

    async fn connect(&mut self) -> Vec<RigEvent> {
        let attempt = tokio::time::timeout(self.timeout, Client::connect(&self.addr)).await;
        match attempt {
            Ok(Ok(client)) => {
                self.client = Some(client);
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

    async fn poll_state(&mut self) -> Vec<RigEvent> {
        let mut events = Vec::new();
        match self.call(async |c| c.get_freq().await).await {
            Ok(hz) => events.push(RigEvent::Freq(hz)),
            Err(Failure::Lost(e)) => return vec![RigEvent::Disconnected(e)],
            Err(Failure::Refused(_)) => {}
        }
        match self.call(async |c| c.get_mode().await).await {
            Ok((mode, passband)) => events.push(RigEvent::Mode {
                mode: RigMode::parse(&mode),
                passband,
            }),
            Err(Failure::Lost(e)) => events.push(RigEvent::Disconnected(e)),
            Err(Failure::Refused(_)) => {}
        }
        events
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
            Err(Failure::Lost(e)) => vec![RigEvent::Disconnected(e)],
        }
    }
}

enum Failure {
    Refused(String),
    Lost(String),
}
