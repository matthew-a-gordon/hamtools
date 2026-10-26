//! A scripted stand-in for `rigctld` serving a KX3. The dummy rig can't emulate `TB;`, so this
//! replays the protocol framing recorded from `rigctld -m 1` (extended `+` mode, including the
//! stray `\r\0` after raw `w` replies) with canned KX3 answers.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

pub struct FakeRigctld {
    pub addr: SocketAddr,
    /// Every command line received, e.g. `+w TB;` or `+b CQ`.
    pub received: Arc<Mutex<Vec<String>>>,
}

impl FakeRigctld {
    /// `tb_replies` are returned, in order, for successive `TB;` polls; after that `TB000;`.
    pub async fn start(tb_replies: &[&str]) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let received = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&received);
        let mut replies: Vec<String> = tb_replies.iter().rev().map(|s| s.to_string()).collect();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (read, mut write) = stream.into_split();
            let mut lines = BufReader::new(read).lines();
            while let Some(line) = lines.next_line().await.unwrap() {
                log.lock().unwrap().push(line.clone());
                let response = if line == "+w TB;" {
                    let reply = replies.pop().unwrap_or_else(|| "TB000;".to_string());
                    format!("send_cmd: TB;\nReply: {reply}\r\0RPRT 0\n")
                } else if let Some(text) = line.strip_prefix("+b ") {
                    format!("send_morse: {text}\nRPRT 0\n")
                } else {
                    "RPRT -11\n".to_string()
                };
                write.write_all(response.as_bytes()).await.unwrap();
            }
        });
        Self { addr, received }
    }
}
