use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, StreamExt};
use tracing::debug;

pub struct ClientConnection {}

impl ClientConnection {
    pub fn new(socket: WebSocket) {
        let (mut sink, mut stream) = socket.split();
        tokio::spawn(async move {
            while let Some(Ok(message)) = stream.next().await {
                match message {
                    Message::Text(txt) => {
                        debug!("收到消息: {:?}", txt)
                    }
                    Message::Binary(bin) => {
                        debug!("收到消息: {:?}", bin)
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
    }
}
