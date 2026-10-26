//! A recording stand-in for `rigctld`: answers every command `RPRT 0` (or `RPRT -1` for lines
//! starting with the configured failing prefix) and keeps the command lines it received.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

pub struct FakeRigctld {
    pub addr: SocketAddr,
    pub received: Arc<Mutex<Vec<String>>>,
}

impl FakeRigctld {
    pub async fn start() -> Self {
        Self::start_failing_on("\u{0}never").await
    }

    /// Lines beginning with `fail_prefix` get `RPRT -1`.
    pub async fn start_failing_on(fail_prefix: &'static str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let received = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&received);
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (read, mut write) = stream.into_split();
            let mut lines = BufReader::new(read).lines();
            while let Some(line) = lines.next_line().await.unwrap() {
                log.lock().unwrap().push(line.clone());
                let code = if line.starts_with(fail_prefix) { -1 } else { 0 };
                let response = format!("echo: {line}\nRPRT {code}\n");
                write.write_all(response.as_bytes()).await.unwrap();
            }
        });
        Self { addr, received }
    }

    pub fn lines(&self) -> Vec<String> {
        self.received.lock().unwrap().clone()
    }
}
