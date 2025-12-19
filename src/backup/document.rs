use crate::backup::errors::ServerError;
use crate::backup::messages::ServerMessage;
use crate::backup::types::{AwarenessPayload, ChangePayload, HookContext, StatelessPayload};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc::Sender;
use uuid::Uuid;
use yrs::sync::Awareness;
use yrs::{Doc, ReadTxn, StateVector, Transact, Update, updates::decoder::Decode};

#[derive(Clone, Debug)]
pub struct DocumentHandle{
    inner: Arc<Document>,
}

impl DocumentHandle {
    pub fn new(document: Document) -> DocumentHandle {
        DocumentHandle {
            inner: Arc::new(document),
        }
    }

    pub fn doc(&self) -> Arc<Document> {
        self.inner.clone()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DocumentUpdate {
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct ConnectionHandle {
    pub socket_id: Uuid,
    pub sender: Sender<ServerMessage>,
    pub context: HookContext,
}

#[derive(Debug)]
pub struct Document {
    name: String,
    awareness: RwLock<Awareness>,
    connections: RwLock<HashMap<Uuid, ConnectionHandle>>,
    direct_connections: RwLock<usize>,
    last_change: RwLock<u128>,
}

impl Document {
    pub fn new(name: impl Into<String>) -> DocumentHandle {
        DocumentHandle::new(Document {
            name: name.into(),
            awareness: RwLock::new(Awareness::new(Doc::new())),
            connections: RwLock::new(HashMap::new()),
            direct_connections: RwLock::new(0),
            last_change: RwLock::new(0),
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn add_connection(&self, handle: ConnectionHandle) {
        self.connections.write().insert(handle.socket_id, handle);
    }

    pub fn remove_connection(&self, id: &Uuid) {
        self.connections.write().remove(id);
    }

    pub fn connection_count(&self) -> usize {
        let sockets = self.connections.read().len();
        let direct = *self.direct_connections.read();
        sockets + direct
    }

    pub fn apply_update(&self, update: &[u8]) -> Result<(), ServerError> {
        let update = Update::decode_v1(update).unwrap();
        let _ = self
            .awareness
            .write()
            .doc()
            .transact_mut()
            .apply_update(update);
        *self.last_change.write() = chrono::Utc::now().timestamp_millis() as u128;
        Ok(())
    }

    pub fn encode_state_as_update(&self, sv: &StateVector) -> Vec<u8> {
        let doc = self.awareness.read().doc().clone();
        let txn = doc.transact();
        txn.encode_state_as_update_v1(sv)
    }

    pub fn encode_state_vector(&self) -> StateVector {
        let doc = self.awareness.read().doc().clone();
        doc.transact().state_vector()
    }

    pub fn broadcast(&self, message: ServerMessage) {
        let connections = self.connections.read().clone();
        for (_id, conn) in connections {
            let _ = conn.sender.send(message.clone());
        }
    }

    pub fn set_awareness(
        &self,
        document: &DocumentHandle,
        client_id: u64,
        state: serde_json::Value,
        context: HookContext,
    ) -> AwarenessPayload {
        AwarenessPayload {
            context,
            document: document.clone(),
            client_id,
            state,
        }
    }

    pub fn awareness_states(&self) -> Vec<(u64, serde_json::Value)> {
        // todo 清空
        vec![]
    }

    pub fn create_change_payload(
        document: &DocumentHandle,
        context: HookContext,
        update: Vec<u8>,
    ) -> ChangePayload {
        ChangePayload {
            context,
            document: document.clone(),
            update: DocumentUpdate { bytes: update },
        }
    }

    pub fn create_stateless_payload(
        document: &DocumentHandle,
        context: HookContext,
        payload: serde_json::Value,
    ) -> StatelessPayload {
        StatelessPayload {
            context,
            document: document.clone(),
            payload,
        }
    }

    pub fn add_direct(&self) {
        *self.direct_connections.write() += 1;
    }

    pub fn remove_direct(&self) {
        let mut guard = self.direct_connections.write();
        if *guard > 0 {
            *guard -= 1;
        }
    }

    pub fn is_empty(&self) -> bool {
        let doc = self.awareness.read().doc().clone();
        let txn = doc.transact();
        let root = txn.get_map("root");
        root.iter().next().is_none()
    }
}

pub struct DirectDocumentConnection {
    document: DocumentHandle,
    context: HookContext,
}

impl DirectDocumentConnection {
    pub fn new(document: DocumentHandle, context: HookContext) -> Self {
        document.inner.add_direct();
        Self { document, context }
    }

    pub fn document(&self) -> DocumentHandle {
        self.document.clone()
    }

    pub fn context(&self) -> &HookContext {
        &self.context
    }

    pub fn close(&self) {
        self.document.inner.remove_direct();
    }
}
