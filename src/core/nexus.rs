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
    pub fn new(setting: &NexusSetting, mut extensions: Vec<Arc<dyn Extension>>) -> Self {
        let moka = setting.clone().moka_setting;
        let channels = Cache::builder()
            .max_capacity(moka.capacity)
            .time_to_idle(Duration::from_secs(moka.time_to_idle))
            .build();
        
        // 按优先级排序扩展（优先级高的先执行）
        extensions.sort_by(|a, b| {
            let pa = a.priority().unwrap_or(100);
            let pb = b.priority().unwrap_or(100);
            pa.cmp(&pb)
        });
        
        debug!("Extensions loaded: {:?}", extensions.iter().map(|e| {
            format!("{}(priority={})", e.name().unwrap_or("unknown"), e.priority().unwrap_or(100))
        }).collect::<Vec<_>>());
        
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
        
        tokio::spawn(async move {
            debounce
                .debounce(debounce_id, delay, max_delay, move || {
                    let extensions = extensions.clone();
                    async move {
                        let state = document
                            .awareness()
                            .read()
                            .await
                            .doc()
                            .transact()
                            .encode_state_as_update_v1(&StateVector::default());
                        let payload = Arc::new(OnStoreDocumentPayload {
                            doc_id: doc_id.clone(),
                            state,
                        });
                        
                        let tasks: Vec<_> = extensions
                            .iter()
                            .map(|ext| {
                                let payload = payload.clone();
                                let ext = ext.clone();
                                tokio::spawn(async move {
                                    if let Err(e) = ext.on_store_document((*payload).clone()).await {
                                        error!("[{}] on_store_document error: {:?}", 
                                            ext.name().unwrap_or("unknown"), e);
                                    }
                                })
                            })
                            .collect();
                        
                        for task in tasks {
                            let _ = task.await;
                        }
                    }
                })
                .await;
        });
    }

    pub async fn handle_connection(&self, socket: WebSocket, doc_id: &str) {
        let doc_id = doc_id.to_string();
        let mut context = HookContext {
            doc_id: doc_id.clone(),
            read_only: false,
            authenticated: false,
            token: None,
        };

        let extensions = self.extensions.clone();
        let ctx = context.clone();
        tokio::spawn(async move {
            let tasks: Vec<_> = extensions
                .iter()
                .map(|ext| {
                    let ctx = ctx.clone();
                    let ext = ext.clone();
                    tokio::spawn(async move {
                        if let Err(e) = ext.on_connect(&ctx).await {
                            error!("[{}] on_connect error: {:?}", ext.name().unwrap_or("unknown"), e);
                        }
                    })
                })
                .collect();
            for task in tasks {
                let _ = task.await;
            }
        });

        if !self.authorization(&context).await.unwrap_or(false) {
            let _ = self
                .disconnect(&doc_id, &context)
                .await
                .map_err(|e| error!("disconnect error: {:?}", e));
            return;
        }
        context.authenticated = true;
        let document = self.get_or_init_document(&doc_id).await;
        let (sink, stream) = socket.split();
        let sink = Arc::new(Mutex::new(AxumSink::from(sink)));
        let stream = AxumStream::from(stream);
        let connection = document.subscribe(sink, stream).await;
        match connection.completed().await {
            Ok(_) => {
                debug!("客户端连接正常关闭");
                let _ = self.disconnect(&doc_id, &context).await;
            }
            Err(e) => {
                error!("客户端连接异常关闭: {:?}", e);
                let _ = self.disconnect(&doc_id, &context).await;
            }
        }
    }

    async fn disconnect(&self, doc_id: &str, context: &HookContext) -> Result<(), Error> {
        let tasks: Vec<_> = self.extensions
            .iter()
            .map(|ext| {
                let ctx = context.clone();
                let ext = ext.clone();
                let doc_id = doc_id.to_string();
                tokio::spawn(async move {
                    if let Err(e) = ext.on_disconnect(&ctx).await {
                        error!("[{}] on_disconnect error for doc {}: {:?}", 
                            ext.name().unwrap_or("unknown"), doc_id, e);
                    }
                })
            })
            .collect();
        
        for task in tasks {
            let _ = task.await;
        }
        Ok(())
    }

    async fn on_change(&self, payload: ChangePayload) -> Result<(), Error> {
        let payload = Arc::new(payload);

        let tasks: Vec<_> = self.extensions
            .iter()
            .map(|ext| {
                let payload = payload.clone();
                let ext = ext.clone();
                tokio::spawn(async move {
                    if let Err(e) = ext.on_change((*payload).clone()).await {
                        error!("[{}] on_change error: {:?}",
                            ext.name().unwrap_or("unknown"), e);
                    }
                })
            })
            .collect();

        for task in tasks {
            let _ = task.await;
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
                let document = Arc::new(document);
                
                let _ = self
                    .load_document(document.clone())
                    .await
                    .map_err(|e| error!("load_document error: {:?}", e));
                
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
                
                document
            })
            .await
    }
}
