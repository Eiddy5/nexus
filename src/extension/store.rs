use crate::core::types::{Extension, OnChangePayload};
use anyhow::Error;
use async_trait::async_trait;

pub struct KVStoreExtension;

impl KVStoreExtension {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl Extension for KVStoreExtension {
    fn priority(&self) -> Option<u8> {
        Some(1)
    }

    fn name(&self) -> Option<&str> {
        Some("store")
    }

    async fn on_change(&self, payload: OnChangePayload) -> anyhow::Result<(), Error> {
        todo!()
    }
}
