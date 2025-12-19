use crate::core::document::Document;
use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::debug;
use yrs::Update;
use yrs::updates::decoder::Decode;

pub struct ClientConnection {
    connections: Arc<Mutex<HashMap<String, Connection>>>,
    callbacks: Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

impl ClientConnection {
    pub fn new(socket: WebSocket) ->Self{
        let (mut sink, mut stream) = socket.split();
        tokio::spawn(async move {
            while let Some(Ok(message)) = stream.next().await {
                match message {
                    Message::Text(txt) => {
                        debug!("收到消息: {:?}", txt)
                    }
                    Message::Binary(bin) => {
                        let msg = yrs::sync::Message::decode_v1(&bin.to_vec()).unwrap();
                        debug!("收到消息: {:?}", msg)
                    }
                    Message::Ping(pin) => {
                        debug!("收到Ping,发送Pong");
                        if let Err(e) = sink.send(Message::Pong(pin)).await {
                            debug!("发送Pong失败: {:?}", e);
                            break;
                        }
                    }
                    Message::Pong(pon) => {
                        debug!("收到Pong {:?}", pon);
                    }
                    Message::Close(_) => {
                        if let Err(e) = sink.close().await {
                            debug!("关闭连接失败: {:?}", e)
                        }
                        debug!("客户端正常断开连接!");
                        break;
                    }
                }
            }
        });
        Self {
            connections: Arc::new(Mutex::new(HashMap::new())),
            callbacks: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}



struct Connection {
    socket: WebSocket,
    document: Document,
}
