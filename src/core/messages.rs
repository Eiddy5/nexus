use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
	Sync { document: String, update: String },
	Awareness { document: String, client_id: u64, state: serde_json::Value },
	Stateless { document: String, payload: serde_json::Value },
	Token { document: String, token: String },
	QueryAwareness { document: String },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
	Sync { document: String, update: String },
	Awareness { document: String, states: Vec<(u64, serde_json::Value)> },
	Stateless { document: String, payload: serde_json::Value },
	TokenRequest { document: String },
	Close { document: String, reason: String },
	SyncStatus { document: String, ok: bool },
}

