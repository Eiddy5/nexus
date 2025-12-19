use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::log::info;
use crate::backup::{default_configuration, Configuration, DirectDocumentConnection, DocumentHandle, ServerError};
use crate::backup::debounce::Debouncer;
use crate::backup::document::Document;
use crate::backup::messages::ServerMessage;
use crate::backup::types::{AwarenessPayload, ChangePayload, HookContext, StatelessPayload};

#[derive(Clone)]
pub struct Nexus {
	pub configuration: Configuration,
	documents: Arc<RwLock<HashMap<String, DocumentHandle>>>,
	debounce: Arc<Debouncer>,
}

impl Nexus {
	pub fn new(configuration: Option<Configuration>) -> Self {
		let mut cfg = default_configuration();
		if let Some(custom) = configuration {
			cfg = custom;
		}

		Self {
			configuration: cfg,
			documents: Arc::new(RwLock::new(HashMap::new())),
			debounce: Arc::new(Debouncer::new()),
		}
	}

	pub async fn get_or_create_document(&self, name: &str) -> DocumentHandle {
		if let Some(existing) = self.documents.read().await.get(name) {
			return existing.clone();
		}
		let mut guard = self.documents.write().await;
		let entry = guard.entry(name.to_string()).or_insert_with(|| Document::new(name));
		entry.clone()
	}

	pub async fn open_direct_connection(
		&self,
		document_name: &str,
		context: HookContext,
	) -> DirectDocumentConnection {
		let doc = self.get_or_create_document(document_name).await;
		DirectDocumentConnection::new(doc, context)
	}

	pub async fn on_connect(&self, context: &HookContext) -> Result<(), ServerError> {
		for extension in &self.configuration.extensions {
			extension.on_connect(context).await?;
		}
		Ok(())
	}

	pub async fn on_authenticate(&self, context: &HookContext) -> Result<(), ServerError> {
		for extension in &self.configuration.extensions {
			if !extension.on_authenticate(context).await? {
				return Err(ServerError::InvalidMessage(
					"authentication failed by extension".into(),
				));
			}
		}
		Ok(())
	}

	pub async fn on_change(&self, payload: &ChangePayload) -> Result<(), ServerError> {
		for extension in &self.configuration.extensions {
			extension.on_change(payload).await?;
		}
		Ok(())
	}

	pub async fn on_stateless(&self, payload: &StatelessPayload) -> Result<(), ServerError> {
		for extension in &self.configuration.extensions {
			extension.on_stateless(payload).await?;
		}
		Ok(())
	}

	pub async fn on_awareness_update(
		&self,
		payload: &AwarenessPayload,
	) -> Result<(), ServerError> {
		for extension in &self.configuration.extensions {
			extension.on_awareness_update(payload).await?;
		}
		Ok(())
	}

	pub async fn on_disconnect(&self, context: &HookContext) -> Result<(), ServerError> {
		for extension in &self.configuration.extensions {
			extension.on_disconnect(context).await?;
		}
		Ok(())
	}

	pub async fn documents_count(&self) -> usize {
		self.documents.read().await.len()
	}

	pub async fn connections_count(&self) -> usize {
		let guard = self.documents.read().await;
		guard
			.values()
			.map(|doc| doc.doc().connection_count())
			.sum()
	}

	pub async fn unload_document(&self, name: &str) {
		let mut guard = self.documents.write().await;
		if let Some(doc) = guard.get(name) {
			if doc.doc().connection_count() == 0 {
				guard.remove(name);
			}
		}
	}

	pub async fn close_connections(&self, document_name: Option<&str>) {
		let guard = self.documents.read().await;
		for (name, doc) in guard.iter() {
			if document_name.map(|d| d == name).unwrap_or(true) {
				doc.doc().broadcast(ServerMessage::Close {
					document: name.clone(),
					reason: "server closed connection".into(),
				});
			}
		}
	}

	pub fn debounce(&self) -> Arc<Debouncer> {
		Arc::clone(&self.debounce)
	}

	pub fn log_start(&self, port: u16, address: &str) {
		if self.configuration.quiet {
			return;
		}
		let name = self
			.configuration
			.name
			.clone()
			.unwrap_or_else(|| "nexus-rust".into());
		info!("nexus {name} listening on http://{address}:{port}");
	}
}

