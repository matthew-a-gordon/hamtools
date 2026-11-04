//! Async client for the Hamlib `rigctld` text protocol.
//!
//! Uses rigctld's extended response protocol (commands prefixed with `+`), where every reply is
//! a header line, `Key: value` lines, and a closing `RPRT <code>` line.

use std::fmt;
use std::io;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpStream, ToSocketAddrs};

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    /// rigctld answered `RPRT <code>` with a negative Hamlib error code.
    Rig(i32),
    /// The reply did not have the expected shape.
    Protocol(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "i/o error: {e}"),
            Error::Rig(code) => write!(f, "rigctld error {code}"),
            Error::Protocol(msg) => write!(f, "protocol error: {msg}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// A connection to one `rigctld`.
pub struct Client {
    stream: BufReader<TcpStream>,
}

impl Client {
    pub async fn connect(addr: impl ToSocketAddrs) -> Result<Self> {
        Ok(Self {
            stream: BufReader::new(TcpStream::connect(addr).await?),
        })
    }

    /// Frequency in Hz.
    pub async fn get_freq(&mut self) -> Result<u64> {
        let reply = self.request("f").await?;
        parse_field(&reply, "Frequency")
    }

    pub async fn set_freq(&mut self, hz: u64) -> Result<()> {
        self.request(&format!("F {hz}")).await?;
        Ok(())
    }

    /// Mode name (e.g. `USB`, `CW`) and passband width in Hz.
    pub async fn get_mode(&mut self) -> Result<(String, i64)> {
        let reply = self.request("m").await?;
        Ok((
            parse_field(&reply, "Mode")?,
            parse_field(&reply, "Passband")?,
        ))
    }

    pub async fn set_mode(&mut self, mode: &str, passband_hz: i64) -> Result<()> {
        self.request(&format!("M {mode} {passband_hz}")).await?;
        Ok(())
    }

    /// Current VFO name (e.g. `VFOA`, `VFOB`).
    pub async fn get_vfo(&mut self) -> Result<String> {
        let reply = self.request("\\get_vfo").await?;
        parse_field(&reply, "VFO")
    }

    pub async fn set_vfo(&mut self, vfo: &str) -> Result<()> {
        self.request(&format!("\\set_vfo {vfo}")).await?;
        Ok(())
    }

    /// Whether split is enabled, and the VFO used for transmit.
    pub async fn get_split(&mut self) -> Result<(bool, String)> {
        let reply = self.request("s").await?;
        let enabled: u8 = parse_field(&reply, "Split")?;
        Ok((enabled != 0, parse_field(&reply, "TX VFO")?))
    }

    pub async fn set_split(&mut self, enabled: bool, tx_vfo: &str) -> Result<()> {
        self.request(&format!("S {} {tx_vfo}", u8::from(enabled)))
            .await?;
        Ok(())
    }

    /// Transmit frequency in Hz while split is on (`i`).
    pub async fn get_split_freq(&mut self) -> Result<u64> {
        let reply = self.request("i").await?;
        parse_field(&reply, "TX Frequency")
    }

    pub async fn set_split_freq(&mut self, hz: u64) -> Result<()> {
        self.request(&format!("I {hz}")).await?;
        Ok(())
    }

    /// Transmit mode and passband width in Hz while split is on (`x`).
    pub async fn get_split_mode(&mut self) -> Result<(String, i64)> {
        let reply = self.request("x").await?;
        Ok((
            parse_field(&reply, "TX Mode")?,
            parse_field(&reply, "TX Passband")?,
        ))
    }

    pub async fn set_split_mode(&mut self, mode: &str, passband_hz: i64) -> Result<()> {
        self.request(&format!("X {mode} {passband_hz}")).await?;
        Ok(())
    }

    /// A level such as `STRENGTH` (the S-meter: a signed dB value relative to S9), as the string
    /// rigctld reports it. Callers parse it; this crate takes on no rig semantics.
    pub async fn get_level(&mut self, name: &str) -> Result<String> {
        let reply = self.request(&format!("l {name}")).await?;
        // Extended mode answers with the header and then the bare value, with no `Key:` label.
        reply
            .lines
            .get(1)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .ok_or_else(|| Error::Protocol(format!("no level value in {:?}", reply.lines)))
    }

    /// Read PTT. Reading cannot key the transmitter.
    pub async fn get_ptt(&mut self) -> Result<bool> {
        let reply = self.request("t").await?;
        let state: u8 = parse_field(&reply, "PTT")?;
        Ok(state != 0)
    }

    pub async fn set_ptt(&mut self, on: bool) -> Result<()> {
        self.request(&format!("T {}", u8::from(on))).await?;
        Ok(())
    }

    /// Send text as CW using the rig's keyer (`b`). Rigs without Hamlib morse support return
    /// [`Error::Rig`].
    pub async fn send_morse(&mut self, text: &str) -> Result<()> {
        self.request(&format!("b {text}")).await?;
        Ok(())
    }

    /// Send a raw rig command (`w`) and return the rig's reply with trailing CR/NUL stripped.
    /// For Kenwood-style rigs such as the KX3 this is the CAT string, e.g. `TB;`.
    pub async fn send_cmd(&mut self, cmd: &str) -> Result<String> {
        let reply = self.request(&format!("w {cmd}")).await?;
        let raw = reply
            .field("Reply")
            .ok_or_else(|| Error::Protocol("missing field \"Reply\"".into()))?;
        Ok(raw.trim_end_matches(['\r', '\0', '\n']).to_string())
    }

    /// Send one command (without the `+` prefix) and read the reply up to its `RPRT` line.
    async fn request(&mut self, command: &str) -> Result<Reply> {
        self.stream
            .get_mut()
            .write_all(format!("+{command}\n").as_bytes())
            .await?;
        let mut lines = Vec::new();
        let mut buf = Vec::new();
        loop {
            buf.clear();
            if self.stream.read_until(b'\n', &mut buf).await? == 0 {
                return Err(Error::Protocol("connection closed mid-reply".into()));
            }
            let line = String::from_utf8_lossy(&buf);
            let line = line.trim_end_matches('\n');
            // Raw replies (`w`) can end without a newline before `RPRT`, so search the line tail.
            if let Some(i) = line.rfind("RPRT ")
                && let Ok(code) = line[i + 5..].trim().parse::<i32>()
            {
                if code < 0 {
                    return Err(Error::Rig(code));
                }
                if i > 0 {
                    lines.push(line[..i].to_string());
                }
                return Ok(Reply { lines });
            }
            lines.push(line.to_string());
        }
    }
}

struct Reply {
    lines: Vec<String>,
}

impl Reply {
    fn field(&self, key: &str) -> Option<&str> {
        self.lines.iter().find_map(|l| {
            let (k, v) = l.split_once(':')?;
            (k == key).then(|| v.trim())
        })
    }
}

fn parse_field<T: std::str::FromStr>(reply: &Reply, key: &str) -> Result<T> {
    let raw = reply
        .field(key)
        .ok_or_else(|| Error::Protocol(format!("missing field {key:?}")))?;
    raw.parse()
        .map_err(|_| Error::Protocol(format!("bad {key} value {raw:?}")))
}
