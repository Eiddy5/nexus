use crate::config::NexusSetting;
use crate::core::connection::{AxumSink, AxumStream};
use crate::core::debounce::Debouncer;
use crate::core::document::Document;
use crate::core::types::{Extension, OnStoreDocumentPayload};
use anyhow::Error;
use axum::extract::ws::WebSocket;
use futures_util::StreamExt;
use moka::future::Cache;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, error};
use yrs::Doc;
use yrs::sync::Awareness;

#[derive(Clone)]
pub struct Nexus {
    documents: Cache<String, Arc<Document>>,
    debounce: Arc<Debouncer>,
    pub extensions: Vec<Arc<dyn Extension>>,
}

impl Nexus {
    pub fn new(setting: &NexusSetting, extensions: Vec<Arc<dyn Extension>>) -> Self {
        let moka = setting.clone().moka_setting;
        let channels = Cache::builder()
            .max_capacity(moka.capacity)
            .time_to_idle(Duration::from_secs(moka.time_to_idle))
            .build();
        Nexus {
            documents: channels,
            debounce: Arc::new(Debouncer::new()),
            extensions,
        }
    }

    pub async fn handle_connection(&self, socket: WebSocket, doc_id: &str) {
        let document = self.get_or_init_document(&doc_id).await;
        let (sink, stream) = socket.split();
        let sink = Arc::new(Mutex::new(AxumSink::from(sink)));
        let stream = AxumStream::from(stream);
        let connection = document.subscribe(sink, stream).await;
        match connection.completed().await {
            Ok(_) => {
                debug!("客户端连接正常关闭");
                let _ = self.on_disconnect(&doc_id).await;
            }
            Err(e) => {
                error!("客户端连接异常关闭: {:?}", e);
                let _ = self.on_disconnect(&doc_id).await;
            }
        }
    }

    async fn on_disconnect(&self, doc_id: &str) -> Result<(), Error> {
        let payload = OnStoreDocumentPayload {};
        for extension in &self.extensions {
            debug!("extension: {:?}", extension.name().unwrap());
            extension.on_store_document(payload.clone()).await?;
        }
        Ok(())
    }

    async fn get_or_init_document(&self, doc_id: &str) -> Arc<Document> {
        self.documents
            .get_with(doc_id.to_string(), async move {
                debug!("init document: {:}", doc_id);
                let doc = Doc::new();
                let awareness = Arc::new(RwLock::new(Awareness::new(doc)));
                let document = Document::new(awareness).await;
                Arc::new(document)
            })
            .await
    }
}
