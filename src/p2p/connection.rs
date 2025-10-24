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
        mut _socket: tokio::net::TcpStream,
        _cancel_token: Arc<CancellationToken>,
    ) {
        let mut buf = [0u8; 1024];
        tokio::select! {
                _ = _cancel_token.cancelled() => {
                    println!("Connection handler stopping due to cancellation");
                    return;
                }
            res = _socket.read(&mut buf) => {
                    match res {
                        Ok(n) if n == 0 => {
                            println!("Connection closed by peer");
                            return;
                        }
                        Ok(n) => {
                            let bytes = &buf[..n];
                            match std::str::from_utf8(bytes) {
                                Ok(s) => {
                                    println!("Received: {}", s);
                                }
                                Err(e) => {
                                    eprintln!("Failed to parse UTF-8: {:?}", e);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to read from socket: {:?}", e);
                            return;
                        }
                }
            }
        }
    }
}
