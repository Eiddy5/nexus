use async_trait::async_trait;
use std::net::SocketAddr;
use std::sync::Arc;
use uuid::Uuid;
use crate::core::document::DocumentUpdate;
use crate::core::DocumentHandle;

#[derive(Clone, Debug)]
pub struct HookContext {
    pub document_name: String,
    pub socket_id: Uuid,
    pub peer: Option<SocketAddr>,
    pub read_only: bool,
    pub authenticated: bool,
    pub token: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ChangePayload {
    pub context: HookContext,
    pub document: DocumentHandle,
    pub update: DocumentUpdate,
}

#[derive(Clone, Debug)]
pub struct StatelessPayload {
    pub context: HookContext,
    pub document: DocumentHandle,
    pub payload: serde_json::Value,
}

#[derive(Clone, Debug)]
pub struct AwarenessPayload {
    pub context: HookContext,
    pub document: DocumentHandle,
    pub client_id: u64,
    pub state: serde_json::Value,
}

#[async_trait]
pub trait Extension: Send + Sync {
    async fn on_configure(&self, _configuration_name: Option<String>) -> anyhow::Result<()> {
        Ok(())
    }

    async fn on_connect(&self, _context: &HookContext) -> anyhow::Result<()> {
        Ok(())
    }

    async fn on_authenticate(&self, _context: &HookContext) -> anyhow::Result<bool> {
        Ok(true)
    }

    async fn on_change(&self, _payload: &ChangePayload) -> anyhow::Result<()> {
        Ok(())
    }

    async fn on_awareness_update(&self, _payload: &AwarenessPayload) -> anyhow::Result<()> {
        Ok(())
    }

    async fn on_stateless(&self, _payload: &StatelessPayload) -> anyhow::Result<()> {
        Ok(())
    }

    async fn on_disconnect(&self, _context: &HookContext) -> anyhow::Result<()> {
        Ok(())
    }
}

pub type SharedExtension = Arc<dyn Extension>;
