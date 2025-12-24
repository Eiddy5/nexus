use crate::core::types::{Extension, ChangePayload};
use anyhow::Error;
use async_trait::async_trait;
use tracing::{debug, info};
use yrs::Update;
use yrs::updates::decoder::Decode;

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

    async fn on_change(&self, payload: ChangePayload) -> anyhow::Result<(), Error> {
        let update = Update::decode_v1(&payload.update)?;
        info!("调用 on_change方法 {:}",update);
        Ok(())
    }
}
