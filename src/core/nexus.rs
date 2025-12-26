use crate::config::NexusSetting;
use crate::core::connection::{AxumSink, AxumStream};
use crate::core::debounce::Debouncer;
use crate::core::document::Document;
use crate::core::types::{
    ChangePayload, Extension, HookContext, OnLoadDocumentPayload, OnStoreDocumentPayload,
};
use anyhow::Error;
use axum::extract::ws::WebSocket;
use futures_util::StreamExt;
use moka::future::Cache;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, error};
use uuid::Uuid;
use yrs::sync::Awareness;
use yrs::updates::encoder::Encode;
use yrs::{Doc, ReadTxn, StateVector, Transact};

#[derive(Clone)]
pub struct Nexus {
    documents: Cache<String, Arc<Document>>,
    debouncer: Arc<Debouncer>,
    debounce_delay: Duration,
    max_debounce: Duration,
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
            debouncer: Arc::new(Debouncer::new()),
            debounce_delay: setting.debounce,
            max_debounce: setting.max_debounce,
            extensions,
        }
    }

    async fn authorization(&self, context: &HookContext) -> Result<bool, Error> {
        let mut authenticated = true;
        for extension in &self.extensions {
            match extension.on_authenticate(&context).await {
                Ok(true) => {}
                Ok(false) => {
                    authenticated = false;
                    break;
                }
                Err(e) => {
                    error!("on_authenticate error: {:?}", e);
                    authenticated = false;
                    break;
                }
            }
        }
        Ok(authenticated)
    }

    async fn store_document(&self, document: Arc<Document>, immediately: Option<bool>) {
        let doc_id = document.doc_id.clone();
        let debounce_id = format!("on_store_document-{}", doc_id);
        let extensions = self.extensions.clone();
        let debounce = self.debouncer.clone();
        let immediately = immediately.unwrap_or(false);
        let delay = if immediately {
            Duration::ZERO
        } else {
            self.debounce_delay
        };
        let max_delay = self.max_debounce;
        debounce
            .debounce(debounce_id, delay, max_delay, move || {
                let extensions = extensions.clone();
                async move {
                    // 获取完整的文档状态作为 Update
                    let state = document
                        .awareness()
                        .read()
                        .await
                        .doc()
                        .transact()
                        .encode_state_as_update_v1(&StateVector::default());
                    let payload = OnStoreDocumentPayload {
                        doc_id: doc_id.clone(),
                        state: state.clone(),
                    };
                    for extension in &extensions {
                        if let Err(e) = extension.on_store_document(payload.clone()).await {
                            error!("on_store_document error: {:?}", e);
                        }
                    }
                }
            })
            .await
    }

    pub async fn handle_connection(&self, socket: WebSocket, doc_id: &str) {
        let document_name = doc_id.to_string();
        let mut context = HookContext {
            document_name: document_name.clone(),
            socket_id: Uuid::new_v4(),
            read_only: false,
            authenticated: false,
            token: None,
        };

        for extension in &self.extensions {
            if let Err(e) = extension.on_connect(&context).await {
                error!("on_connect error: {:?}", e);
            }
        }

        if !self.authorization(&context).await.unwrap_or(false) {
            let _ = self
                .disconnect(doc_id, &context)
                .await
                .map_err(|e| error!("disconnect error: {:?}", e));
            return;
        }
        context.authenticated = true;
        let document = self.get_or_init_document(&document_name).await;
        let (sink, stream) = socket.split();
        let sink = Arc::new(Mutex::new(AxumSink::from(sink)));
        let stream = AxumStream::from(stream);
        let connection = document.subscribe(sink, stream).await;
        match connection.completed().await {
            Ok(_) => {
                debug!("客户端连接正常关闭");
                let _ = self.disconnect(&document_name, &context).await;
            }
            Err(e) => {
                error!("客户端连接异常关闭: {:?}", e);
                let _ = self.disconnect(&document_name, &context).await;
            }
        }
    }

    async fn disconnect(&self, doc_id: &str, context: &HookContext) -> Result<(), Error> {
        for extension in &self.extensions {
            if let Err(e) = extension.on_disconnect(context).await {
                error!("on_disconnect error for doc {}: {:?}", doc_id, e);
            }
        }
        Ok(())
    }

    async fn on_change(&self, payload: ChangePayload) -> Result<(), Error> {
        for extension in &self.extensions {
            extension.on_change(payload.clone()).await?;
        }
        if let Some(document) = self.documents.get(&payload.doc_id).await {
            self.store_document(document.clone(), None).await;
        }
        Ok(())
    }

    async fn load_document(&self, document: Arc<Document>) -> Result<(), Error> {
        let payload = OnLoadDocumentPayload {
            document: document.clone(),
        };
        for extension in &self.extensions {
            extension.on_load_document(payload.clone()).await?;
        }
        Ok(())
    }

    async fn get_or_init_document(&self, doc_id: &str) -> Arc<Document> {
        let doc_id = doc_id.to_string();
        let nexus = self.clone();
        self.documents
            .get_with(doc_id.clone(), async move {
                debug!("init document: {:}", doc_id);
                let doc = Doc::new();
                let awareness = Arc::new(RwLock::new(Awareness::new(doc)));
                let document = Document::new(doc_id.clone(), awareness).await;
                document
                    .on_update(move |update| {
                        let payload = ChangePayload {
                            doc_id: doc_id.clone(),
                            update,
                        };
                        let nexus = nexus.clone();
                        tokio::spawn(async move {
                            let _ = nexus
                                .on_change(payload)
                                .await
                                .map_err(|e| error!("on_change error: {:?}", e));
                        });
                    })
                    .await;
                let document = Arc::new(document);
                let _ = self
                    .load_document(document.clone())
                    .await
                    .map_err(|e| error!("load_document error: {:?}", e));
                document
            })
            .await
    }
}
