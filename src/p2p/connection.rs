use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tokio::{io::AsyncReadExt, net::TcpListener};
use tokio_util::sync::CancellationToken;
pub struct Server {
    inflight_requests: Arc<Mutex<HashMap<String, Arc<CancellationToken>>>>,
}
impl Server {
    pub fn new() -> Self {
        // TODO: load config from config.toml file
        Server {
            inflight_requests: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    pub async fn start(&self, cancel_token: Arc<CancellationToken>) -> tokio::io::Result<()> {
        let listener = TcpListener::bind("0.0.0.0:8082").await.unwrap();
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
                            // generate the request id using correct nano second + the addr
                            let req_id = format!("{}-{}", addr, tokio::time::Instant::now().elapsed().as_nanos());
                            let req_id_clone = req_id.clone();
                            self.inflight_requests.lock().unwrap().insert(req_id, cancel_token.clone());
                            let inflight_requests = self.inflight_requests.clone();
                            println!("Accepted connection from {:?} {:?}", addr, socket);
                            let socket_token = cancel_token.clone();
                            tokio::spawn(async move {
                                Server::handle_connection(socket, socket_token).await;
                                inflight_requests.lock().unwrap().remove(&req_id_clone);
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
        // wait for all inflight requests to finish
        loop {
            let inflight_count = self.inflight_requests.lock().unwrap().len();
            if inflight_count == 0 {
                break;
            }
            println!(
                "Waiting for {} inflight requests to finish...",
                inflight_count
            );
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }
        println!("Server stopped");
    }
    async fn handle_connection(
        mut socket: tokio::net::TcpStream,
        cancel_token: Arc<CancellationToken>,
    ) {
        let mut buf = vec![0u8; 1024];
        let mut read_acc: Vec<u8> = Vec::new();

        loop {
            tokio::select! {
                _ = cancel_token.cancelled() => return,
                res = socket.read(&mut buf) => {
                    let n = match res {
                        Ok(0) => { println!("Connection closed by peer"); return; }
                        Ok(n) => n,
                        Err(e) => { eprintln!("read error: {:?}", e); return; }
                    };

                    read_acc.extend_from_slice(&buf[..n]);

                    while let Some((consumed, command)) = parse_command(&read_acc) {
                        read_acc.drain(..consumed);
                        println!("parsed {:?}", command);

                    }
                }
            }
        }
    }
}

#[derive(Debug)]
struct Command {
    pub name: String,
    pub key: String,
    pub args: HashMap<String, Vec<String>>,
}
fn parse_command(buf: &[u8]) -> Option<(usize, Command)> {
    if buf.is_empty() || buf[0] != b'*' {
        return None; // invalid or empty buffer for parsing to command
    }
    let mut pos = 1; // current possition of readed buf
    let newline_pos = buf.iter().position(|&b| b == b'\r')?;
    let arr_len: usize = str::from_utf8(&buf[pos..newline_pos]).ok()?.parse().ok()?;
    pos = newline_pos + 2; // skip \r\n
    let mut elements = Vec::with_capacity(arr_len);
    for _ in 0..arr_len {
        if buf.get(pos)? != &b'$' {
            return None;
        } // must be bulk string
        pos += 1;

        let newline_pos = buf[pos..].iter().position(|&b| b == b'\r')? + pos;
        let str_len: usize = str::from_utf8(&buf[pos..newline_pos]).ok()?.parse().ok()?;
        pos = newline_pos + 2;
        if buf.len() < pos + str_len + 2 {
            return None;
        } // not enough data yet
        let s = str::from_utf8(&buf[pos..pos + str_len]).ok()?.to_string();
        elements.push(s);
        pos += str_len + 2;
    }
    if elements.len() < 2 {
        return None;
    }
    let name = elements[0].clone();
    let key = elements[1].clone();
    let mut args = HashMap::new();

    for arg in &elements[2..] {
        if let Some((k, v)) = arg.split_once('=') {
            let vals = v.split(',').map(|s| s.to_string()).collect::<Vec<_>>();
            args.insert(k.to_string(), vals);
        }
    }

    Some((pos, Command { name, key, args }))
}
