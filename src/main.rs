mod p2p;
use std::sync::Arc;

use tokio::signal;
use tokio_util::sync::CancellationToken;

use crate::p2p::connection::Server;

#[tokio::main]
async fn main() {
    let cancel_token = Arc::new(CancellationToken::new());
    let shutdown_token = cancel_token.clone();
    tokio::spawn(async move {
        listen_for_shutdown(shutdown_token).await;
    });
    let server_token = cancel_token.clone();
    let server = Server::new();

    tokio::select! {
        _ = server.start(server_token) => {
            println!("Server task completed");
        },
    }
}
async fn listen_for_shutdown(token: Arc<CancellationToken>) {
    let _ = signal::ctrl_c().await;
    println!("received shutdown signal");
    token.cancel();
}
