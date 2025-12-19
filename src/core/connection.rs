use crate::core::document::Document;
use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::debug;
use yrs::Update;
use yrs::sync::{SyncMessage, YMessage};
use yrs::updates::decoder::Decode;
use yrs::updates::encoder::Encode;

pub struct ClientConnection {
    connections: Arc<Mutex<HashMap<String, Connection>>>,
    callbacks: Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

impl ClientConnection {
    pub fn new(doc_id: String, socket: WebSocket) -> Self {
        let (mut sink, mut stream) = socket.split();
        tokio::spawn(async move {
            while let Some(Ok(message)) = stream.next().await {
                match message {
                    Message::Text(txt) => {
                        debug!("收到消息: {:?}", txt)
                    }
                    Message::Binary(bin) => {
                        message_handler(YMessage::decode_v1(&bin.to_vec()).expect("解析失败"));
                    }
                    Message::Ping(pin) => {
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
        debug!("{:} 连接成功!",doc_id);
        Self {
            connections: Arc::new(Mutex::new(HashMap::new())),
            callbacks: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    async fn message_handler(&self, doc_id:&str, message: YMessage) {
        let connection = self.connections.lock().await.get(doc_id);
        if connection.is_none() {
            debug!("未找到文档: {:}", doc_id);
            return;
        }
        match message {
            YMessage::Sync(sync) => match sync {
                SyncMessage::SyncStep1(sv) => {
                    debug!("收到客户端stateVector: {:?}", sv.encode_v1())
                }
                SyncMessage::SyncStep2(update) => {
                    debug!("收到客户端update消息: {:?}", Update::decode_v1(&update))
                }
                SyncMessage::Update(update) => {
                    debug!("收到客户端update消息: {:?}", Update::decode_v1(&update))
                }
            },
            YMessage::Auth(_) => {}
            YMessage::AwarenessQuery => {}
            YMessage::Awareness(_) => {}
            YMessage::Custom(_, _) => {}
        }
    }
}

struct Connection {
    socket: WebSocket,
    document: Document,
}


