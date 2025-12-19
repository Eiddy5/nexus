use crate::ServerError;
use crate::core::config::NexusSetting;
use crate::core::connection::ClientConnection;
use crate::core::debounce::Debouncer;
use crate::core::default_setting;
use crate::core::document::Document;
use crate::core::types::{
    AwarenessPayload, ChangePayload, Extension, HookContext, SharedExtension, StatelessPayload,
};
use axum::extract::ws::WebSocket;
use moka::sync::Cache;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone)]
pub struct Nexus {
    pub setting: NexusSetting,
    documents: Cache<String, Arc<Document>>,
    debounce: Arc<Debouncer>,
    pub extensions: Vec<SharedExtension>,
}

impl Nexus {
    pub fn new(custom: Option<NexusSetting>, mut extensions: Vec<Arc<dyn Extension>>) -> Self {
        let mut setting = default_setting();
        if let Some(custom) = custom {
            setting = custom;
        }
        let documents = Cache::builder()
            .max_capacity(1000000)
            .time_to_idle(Duration::from_millis(86400))
            .build();
        extensions.sort_by_key(|ext| ext.priority());
        Self {
            setting,
            debounce: Arc::new(Debouncer::new()),
            documents,
            extensions,
        }
    }

    pub async fn on_connect(&self, context: &HookContext) -> Result<(), ServerError> {
        for extension in &self.extensions {
            extension.on_connect(context).await?;
        }
        Ok(())
    }
    pub async fn on_authenticate(&self, context: &HookContext) -> Result<(), ServerError> {
        for extension in &self.extensions {
            if !extension.on_authenticate(context).await? {
                return Err(ServerError::InvalidMessage(
                    "authentication failed by extension".into(),
                ));
            }
        }
        Ok(())
    }

    pub async fn on_change(&self, payload: &ChangePayload) -> Result<(), ServerError> {
        for extension in &self.extensions {
            extension.on_change(payload).await?;
        }
        Ok(())
    }

    pub async fn on_stateless(&self, payload: &StatelessPayload) -> Result<(), ServerError> {
        for extension in &self.extensions {
            extension.on_stateless(payload).await?;
        }
        Ok(())
    }

    pub async fn on_awareness_update(&self, payload: &AwarenessPayload) -> Result<(), ServerError> {
        for extension in &self.extensions {
            extension.on_awareness_update(payload).await?;
        }
        Ok(())
    }

    pub async fn on_disconnect(&self, context: &HookContext) -> Result<(), ServerError> {
        for extension in &self.extensions {
            extension.on_disconnect(context).await?;
        }
        Ok(())
    }

    pub async fn handle_connection(&self, doc_id: String, socket: WebSocket) {
        let client_connection = ClientConnection::new(doc_id, socket);
    }
}
