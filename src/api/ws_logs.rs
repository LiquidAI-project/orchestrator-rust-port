use std::net::SocketAddr;
use anyhow::Result;
use futures::{StreamExt, SinkExt};
use mongodb::{bson::{doc, DateTime as BsonDateTime}, Collection};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{broadcast},
    time::{sleep, Duration},
};
use tokio_tungstenite::{
    accept_hdr_async,
    tungstenite::{
        handshake::server::{Request, Response},
        Message,
        http,
    }
};
use chrono::{DateTime, Utc};
use log::{error, info};
use crate::structs::logs::SupervisorLog;


#[derive(Clone)]
pub struct WsHub {
    tx: broadcast::Sender<String>,
}
impl WsHub {
    pub fn new(capacity: usize) -> Self {
        let (tx, _rx) = broadcast::channel(capacity);
        Self { tx }
    }
    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.tx.subscribe()
    }
    pub fn send(&self, msg: String) {
        let _ = self.tx.send(msg);
    }
}

/// Start a WebSocket server that serves at /ws/logs.
pub async fn run_ws_logs_server(addr: SocketAddr, coll: Collection<SupervisorLog>) -> Result<()> {
    let listener = TcpListener::bind(addr).await?;
    info!("WebSocket server listening on {}", addr);
    let hub = WsHub::new(1024);
    tokio::spawn(start_mongo_poller(coll.clone(), hub.clone()));

    loop {
        let (stream, peer) = listener.accept().await?;
        let hub_clone = hub.clone();

        tokio::spawn(async move {
            if let Err(e) = handle_ws_conn(stream, peer, hub_clone).await {
                error!("WS connection error ({}): {:?}", peer, e);
            }
        });
    }

}


/// Accept a single WebSocket connection and stream broadcast messages to it.
async fn handle_ws_conn(stream: TcpStream, peer: SocketAddr, hub: WsHub) -> Result<()> {

    let callback = |req: &Request, mut resp: Response|
        -> std::result::Result<Response, http::Response<Option<String>>> {
        if req.uri().path() != "/ws/logs" {
            *resp.status_mut() = http::StatusCode::NOT_FOUND;
        }
        Ok(resp)
    };

    let ws_stream = accept_hdr_async(stream, callback).await?;
    info!("WS connected: {}", peer);
    let (mut sink, _source) = ws_stream.split();
    let mut rx = hub.subscribe();

    loop {
        tokio::select! {
            item = rx.recv() => {
                match item {
                    Ok(msg) => {
                        if let Err(e) = sink.send(Message::Text(msg)).await {
                            error!("WS send error to {}: {}", peer, e);
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        error!("WS client {} lagged by {} messages", peer, n);
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }

    info!("WS disconnected: {}", peer);
    Ok(())
}


/// Poll MongoDB for new logs and broadcast them to all connected WebSocket clients.
async fn start_mongo_poller(coll: Collection<SupervisorLog>, hub: WsHub) {
    let mut last_checked: DateTime<Utc> = Utc::now();

    loop {
        let filter = doc! {
            "dateReceived": { "$gt": BsonDateTime::from_chrono(last_checked) }
        };
        match coll.find(filter).await {
            Ok(mut cursor) => {
                let mut max_seen = last_checked;

                while let Some(Ok(doc)) = cursor.next().await {
                    let t = doc.date_received;
                    if t > max_seen {
                        max_seen = t;
                    }

                    // Broadcast
                    match serde_json::to_string(&doc) {
                        Ok(json) => hub.send(json),
                        Err(e) => error!("Failed to serialize log to JSON: {}", e),
                    }
                }

                last_checked = max_seen;
            }
            Err(e) => {
                error!("Mongo poll error: {}", e);
            }
        }

        sleep(Duration::from_secs(5)).await;
    }
}