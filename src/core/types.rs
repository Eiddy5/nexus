use std::sync::Arc;
use anyhow::Error;
use async_trait::async_trait;
use crate::core::document::Document;

#[async_trait]
pub trait Extension: Send + Sync {
    fn priority(&self) -> Option<u8>;
    fn name(&self) -> Option<&str>;

    async fn on_connect(&self, _context: &HookContext) -> anyhow::Result<()> {
        Ok(())
    }

    async fn on_authenticate(&self, _context: &HookContext) -> anyhow::Result<bool> {
        Ok(true)
    }

    async fn on_disconnect(&self, _context: &HookContext) -> anyhow::Result<()> {
        Ok(())
    }

    async fn on_store_document(
        &self,
        _payload: OnStoreDocumentPayload,
    ) -> anyhow::Result<(), Error> {
        Ok(())
    }
    
    async fn on_load_document(&self, _payload: OnLoadDocumentPayload) -> Result<(), Error> {
        Ok(())
    }

    async fn on_change(&self, _payload: ChangePayload) -> anyhow::Result<(), Error> {
        Ok(())
    }
}

#[derive(Clone)]
pub struct OnLoadDocumentPayload {
    pub document:Arc<Document>
}


#[derive(Debug, Clone)]
pub struct ChangePayload {
    pub doc_id: String,
    pub update: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct OnStoreDocumentPayload {
    pub(crate) doc_id: String,
    pub(crate) state: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct HookContext {
    pub doc_id: String,
    pub read_only: bool,
    pub authenticated: bool,
    pub token: Option<String>,
}

