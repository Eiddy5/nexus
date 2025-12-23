use crate::core::document::Document;
use moka::future::Cache;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::debug;
use yrs::Doc;
use yrs::sync::Awareness;
use crate::config::NexusSetting;

#[derive(Clone)]
pub struct Nexus {
    documents: Cache<String, Arc<Document>>,
}

impl Nexus {
    pub fn new(setting: &NexusSetting) -> Self {
        let moka = setting.clone().moka_setting;
        let channels = Cache::builder()
            .max_capacity(moka.capacity)
            .time_to_idle(Duration::from_secs(moka.time_to_idle))
            .build();
        Nexus {
            documents: channels,
        }
    }

    pub async fn get_or_init_document(&self, doc_id: &str) -> Arc<Document> {
        debug!("get document: {:}", doc_id);
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
