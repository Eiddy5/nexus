use anyhow::Error;
use async_trait::async_trait;
#[async_trait]
pub trait Extension: Send + Sync {
    fn priority(&self) -> Option<u8>;
    fn name(&self) -> Option<&str>;

    async fn on_disconnect(&self, _payload: OnStoreDocumentPayload) -> anyhow::Result<()> {
        Ok(())
    }
    async fn on_store_document(&self, _payload: OnStoreDocumentPayload) -> anyhow::Result<(), Error> {
        Ok(())
    }
    async fn on_change(&self, payload: ChangePayload) -> anyhow::Result<(), Error>{
        Ok(())
    }
}

#[derive(Debug)]
pub struct OnCreateDocumentPayload {}

#[derive(Debug)]
pub struct OnConnectionPayload {}

pub struct OnLoadDocumentPayload {}

pub struct AfterLoadDocumentPayload {}

#[derive(Debug, Clone)]
pub struct ChangePayload {
    pub doc_id: String,
    pub update: Vec<u8>,
}

#[derive(Copy, Clone, Debug)]
pub struct OnStoreDocumentPayload {}
