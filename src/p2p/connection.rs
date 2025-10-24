use crate::storage::stream::Stream;
use async_trait::async_trait;
use std::collections::HashMap;
use std::error::Error as StdError;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::sync::CancellationToken;

pub struct Server {
    inflight_requests: Arc<Mutex<HashMap<String, Arc<CancellationToken>>>>,
    stream_storage: Arc<Stream>,
}

impl Server {
    pub fn new() -> Self {
        Server {
            inflight_requests: Arc::new(Mutex::new(HashMap::with_capacity(1024))),
            stream_storage: Arc::new(Stream::new()),
        }
    }

    pub async fn start(
        self: Arc<Self>,
        cancel_token: Arc<CancellationToken>,
    ) -> tokio::io::Result<()> {
        let listener = TcpListener::bind("0.0.0.0:8082").await?;

        loop {
            tokio::select! {
                _ = cancel_token.cancelled() => {
                    println!("Server stopping due to cancellation");
                    self.stop().await;
                    break;
                }
                accept_res = listener.accept() => {
                    match accept_res {
                        Ok((socket, addr)) => {
                            let req_id = format!("{}-{}", addr, std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_nanos());

                            let req_id_clone = req_id.clone();
                            self.inflight_requests.lock().unwrap().insert(req_id, cancel_token.clone());

                            println!("Accepted connection from {:?}", addr);


                            let server_clone = Arc::clone(&self);
                            let socket_token = cancel_token.clone();

                            tokio::spawn(async move {
                                server_clone.handle_connection(socket, socket_token).await;
                                server_clone.inflight_requests.lock().unwrap().remove(&req_id_clone);
                            });
                        }
                        Err(e) => {
                            eprintln!("Failed to accept connection: {:?}", e);
                        }
                    }
                }
            }
        }
        self.stop().await;
        Ok(())
    }

    pub async fn stop(&self) {
        let mut sleep_duration = 100;
        loop {
            let inflight_count = self.inflight_requests.lock().unwrap().len();
            if inflight_count == 0 {
                break;
            }
            println!(
                "Waiting for {} inflight requests to finish...",
                inflight_count
            );
            tokio::time::sleep(tokio::time::Duration::from_millis(sleep_duration)).await;
            sleep_duration = (sleep_duration * 2).min(2000);
        }
        println!("Server stopped");
    }

    async fn handle_connection(&self, mut socket: TcpStream, cancel_token: Arc<CancellationToken>) {
        let mut buf = vec![0u8; 4096];
        let mut read_acc: Vec<u8> = Vec::with_capacity(8192);

        socket.set_nodelay(true).ok();

        loop {
            tokio::select! {
                _ = cancel_token.cancelled() => {
                    println!("Connection cancelled");
                    return;
                }
                res = socket.read(&mut buf) => {
                    let n = match res {
                        Ok(0) => {
                            println!("Connection closed by peer");
                            return;
                        }
                        Ok(n) => n,
                        Err(e) => {
                            eprintln!("read error: {:?}", e);
                            return;
                        }
                    };

                    read_acc.extend_from_slice(&buf[..n]);

                    while let Some((consumed, command)) = parse_command(&read_acc) {
                        read_acc.drain(..consumed);


                        let resp = command.execute(Arc::clone(&self.stream_storage)).await;
                        let response_str = match resp {
                            Ok(val) => format!("+{}\r\n", val),
                            Err(e) => format!("-Error: {}\r\n", e),
                        };

                        if let Err(e) = socket.write_all(response_str.as_bytes()).await {
                            eprintln!("write error: {:?}", e);
                            return;
                        }

                        if let Err(e) = socket.flush().await {
                            eprintln!("flush error: {:?}", e);
                            return;
                        }
                    }

                    if read_acc.capacity() > 65536 && read_acc.len() < 4096 {
                        read_acc.shrink_to(8192);
                    }
                }
            }
        }
    }
}

#[async_trait]
pub trait Command: Send + Sync {
    async fn execute(
        &self,
        storage: Arc<Stream>,
    ) -> Result<String, Box<dyn StdError + Send + Sync>>;
}

pub struct GetCommand {
    pub key: String,
    pub args: HashMap<String, Vec<String>>,
}
pub struct SetCommand {
    pub key: String,
    pub value: Vec<u8>,
}

#[async_trait]
impl Command for GetCommand {
    async fn execute(
        &self,
        storage: Arc<Stream>,
    ) -> Result<String, Box<dyn StdError + Send + Sync>> {
        let names = self.args.get("Names").map(|v| v.as_slice()).unwrap_or(&[]);

        let result = storage.get(&self.key, names);
        match result {
            Some(value) => Ok(value),
            None => Ok(String::from("Key not found")),
        }
    }
}

#[async_trait]
impl Command for SetCommand {
    async fn execute(
        &self,
        storage: Arc<Stream>,
    ) -> Result<String, Box<dyn StdError + Send + Sync>> {
        storage.set(self.key.clone(), self.value.clone());
        Ok(String::from("OK"))
    }
}

fn parse_command(buf: &[u8]) -> Option<(usize, Box<dyn Command>)> {
    if buf.is_empty() || buf[0] != b'*' {
        return None;
    }

    let mut pos = 1;

    let newline_pos = memchr::memchr(b'\r', &buf[pos..])?;
    let arr_len: usize = std::str::from_utf8(&buf[pos..pos + newline_pos])
        .ok()?
        .parse()
        .ok()?;
    pos += newline_pos + 2;

    let mut elements = Vec::with_capacity(arr_len);

    for _ in 0..arr_len {
        if buf.get(pos)? != &b'$' {
            return None;
        }
        pos += 1;

        let newline_pos = memchr::memchr(b'\r', &buf[pos..])?;
        let str_len: usize = std::str::from_utf8(&buf[pos..pos + newline_pos])
            .ok()?
            .parse()
            .ok()?;
        pos += newline_pos + 2;

        if buf.len() < pos + str_len + 2 {
            return None;
        }

        let s = std::str::from_utf8(&buf[pos..pos + str_len])
            .ok()?
            .to_string();
        elements.push(s);
        pos += str_len + 2;
    }

    if elements.len() < 2 {
        return None;
    }

    let name = &elements[0];
    let key = elements[1].clone();

    let mut args = HashMap::with_capacity(elements.len().saturating_sub(2));

    for arg in &elements[2..] {
        if let Some(eq_pos) = memchr::memchr(b'=', arg.as_bytes()) {
            let k = &arg[..eq_pos];
            let v = &arg[eq_pos + 1..];

            let vals: Vec<String> = v.split(',').map(|s| s.to_string()).collect();
            args.insert(k.to_string(), vals);
        }
    }

    match name.as_str() {
        "GET" => Some((pos, Box::new(GetCommand { key, args }))),
        _ => None,
    }
}
