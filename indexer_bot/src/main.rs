use log::{error, info, warn};
use futures_util::sink::SinkExt;
use futures_util::stream::StreamExt;
use serde_json::{json, Value};
use std::env;
use std::error::Error;
use tokio::sync::mpsc::{self, Sender};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use dotenv::dotenv;

pub struct SolanaConnector {
    uri: String,
    max_reconnect_attempts: u32,
}

impl SolanaConnector {
    pub fn new(uri: &str, max_reconnect_attempts: u32) -> Self {
        SolanaConnector {
            uri: uri.to_string(),
            max_reconnect_attempts,
        }
    }

    fn prepare_subscribe_msg(&self, program_id: &str) -> Value {
        json!({
            "jsonrpc": "2.0",
            "id": "1",
            "method": "programSubscribe",
            "params": [
                program_id,
                {
                    "encoding": "base64",
                    "commitment": "confirmed"
                }
            ]
        })
    }

    pub async fn connect(&self, program_id: &str, sender: Sender<Value>) -> Result<(), Box<dyn Error>> {
        let mut reconnect_attempts = 0;

        loop {
            let ws_stream = match connect_async(&self.uri).await {
                Ok((stream, response)) => {
                    info!("Connected with response: {:?}", response);
                    stream
                }
                Err(e) => {
                    error!("Failed to connect: {}", e);
                    if reconnect_attempts < self.max_reconnect_attempts {
                        reconnect_attempts += 1;
                        warn!("Reconnection attempt {} of {}", reconnect_attempts, self.max_reconnect_attempts);
                        continue;
                    } else {
                        return Err(Box::new(e));
                    }
                }
            };

            let (mut write, mut read) = ws_stream.split();

            let subscribe_msg = self.prepare_subscribe_msg(program_id);
            if write.send(Message::Text(subscribe_msg.to_string())).await.is_err() {
                error!("Failed to send subscription message.");
                return Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, "Failed to send message")));
            }
            info!("Subscription message sent.");

            while let Some(msg) = read.next().await {
                match msg {
                    Ok(message) => match message {
                        Message::Text(text) => {
                            if let Ok(data) = serde_json::from_str::<Value>(&text) {
                                if data["id"].is_string() && data["jsonrpc"].is_string() && data["result"].is_number() {
                                    info!("Connection success with the Solana WebSocket");
                                } else  {
                                    if sender.send(data).await.is_err() {
                                        warn!("Failed to send message to channel.");
                                    } else {
                                        info!("Message sent to channel.");
                                    }
                                }
                            } else {
                                warn!("Failed to deserialize message.");
                            }
                        }
                        Message::Ping(ping_data) => {
                            if write.send(Message::Pong(ping_data)).await.is_err() {
                                warn!("Failed to send pong.");
                            } else {
                                info!("Sending pong in response to ping.");
                            }
                        }
                        Message::Pong(_) => {
                            info!("Received pong.");
                        }
                        _ => {
                            info!("Received an unhandled message type.");
                        }
                    },
                    Err(e) => {
                        error!("Error reading message: {}", e);
                        if e.to_string() == "WebSocket protocol error: Connection reset without closing handshake" {
                            break;
                        } else {
                            return Err(Box::new(e));
                        }
                    }
                }
            }
            if reconnect_attempts < self.max_reconnect_attempts {
                reconnect_attempts += 1;
                warn!("Reconnection attempt {} of {}", reconnect_attempts, self.max_reconnect_attempts);
            } else {
                return Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, "Max reconnection attempts reached")));
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv().ok();
    env_logger::init();

    let ws_endpoint = env::var("WS_ENDPOINT").expect("WS_ENDPOINT must be set");
    let program_id = env::var("PROGRAM_ID").expect("PROGRAM_ID must be set");
    let max_reconnect_attempts: u32 = env::var("MAX_RECONNECT_ATTEMPTS")
        .expect("MAX_RECONNECT_ATTEMPTS must be set")
        .parse()
        .expect("MAX_RECONNECT_ATTEMPTS must be a valid u32");

    let (tx, mut rx) = mpsc::channel(100);

    let connector = SolanaConnector::new(&ws_endpoint, max_reconnect_attempts);
    tokio::spawn(async move {
        if let Err(e) = connector.connect(&program_id, tx).await {
            error!("Failed to connect: {}", e);
        }
    });

    while let Some(message) = rx.recv().await {
        println!("Received message: {:?}", message);
    }

    Ok(())
}
