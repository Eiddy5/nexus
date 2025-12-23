use crate::core::config::NexusSetting;
use crate::core::document::Document;
use moka::future::Cache;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use yrs::Doc;
use yrs::sync::Awareness;

#[derive(Clone)]
pub struct Nexus {
    documents: Cache<String, Arc<Document>>,
}

impl Nexus {
    pub fn new(setting: &NexusSetting) -> Self {
        let channels = Cache::builder()
            .max_capacity(setting.capacity)
            .time_to_idle(Duration::from_millis(setting.time_to_idle))
            .build();
        Nexus {
            documents: channels,
        }
    }

    pub async fn get_or_init_document(&self, doc_id: &str) -> Arc<Document> {
        self.documents
            .get_with(doc_id.to_string(), async move {
                let doc = Doc::new();
                let awareness = Arc::new(RwLock::new(Awareness::new(doc)));
                let document = Document::new(awareness).await;
                Arc::new(document)
            })
            .await
    }
}
