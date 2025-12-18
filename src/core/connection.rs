use crate::core::errors::ServerError;
use crate::core::types::HookContext;
use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, StreamExt};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::mpsc::channel;
use tracing::error;
use uuid::Uuid;
use yrs::sync::{ SyncMessage};
use crate::core::document::{ConnectionHandle, Document};
use crate::core::{DocumentHandle, Nexus};
use crate::core::messages::{ClientMessage, ServerMessage};

pub async fn attach_websocket(
    nexus: Arc<Nexus>,
    socket: WebSocket,
    document_name: String,
    peer: Option<SocketAddr>,
) -> Result<(), ServerError> {
    let socket_id = Uuid::new_v4();
    let document = nexus.get_or_create_document(&document_name).await;
    let (mut sink, mut stream) = socket.split();
    let (out_tx, mut out_rx) = channel(1024);
    let context = HookContext {
        document_name: document_name.clone(),
        socket_id,
        peer,
        read_only: false,
        authenticated: false,
        token: None,
    };
    let connection_handle = ConnectionHandle {
        socket_id,
        sender: out_tx.clone(),
        context: context.clone(),
    };
    document.doc().add_connection(connection_handle);
    nexus.on_connect(&context).await?;
    let _state = document.doc().encode_state_vector();
    // todo server主动发起同步
    // let _ = out_tx.send(ServerMessage::Sync {
    //     document: document_name.clone(),
    //     update: base64::encode(state),
    // });

    let doc_clone = document.clone();
    let nexus = nexus.clone();
    let recv_task = tokio::spawn(async move {
        while let Some(msg) = stream.next().await {
            match msg {
                Ok(Message::Text(_)) => {}
                Ok(Message::Binary(bin)) => {
                    // if let Err(err) =
                    //     handle_client_message(&nexus, &doc_clone, &context, Vec::from(bin)).await
                    // {
                    //     error!("ws binary handling failed: {err:?}");
                    //     break;
                    // }
                }
                Ok(Message::Close(_)) => break,
                Ok(Message::Ping(_)) => {}
                Ok(Message::Pong(_)) => {}
                Err(err) => {
                    error!("websocket error: {err:?}");
                    break;
                }
            }
        }
    });

    let send_task = tokio::spawn(async move {
        while let Some(message) = out_rx.recv().await {
            match serde_json::to_string(&message) {
                Ok(payload) => {
                    if sink.send(Message::Binary(payload.into())).await.is_err() {
                        break;
                    }
                }
                Err(err) => {
                    error!("failed to serialize server message: {err:?}");
                    break;
                }
            }
        }
    });

    let _ = tokio::join!(recv_task, send_task);

    nexus.on_disconnect(&context).await?;
    document.doc().remove_connection(&socket_id);
    Ok(())
}

async fn handle_client_message(
    nexus: &Arc<Nexus>,
    document: &DocumentHandle,
    context: &HookContext,
    raw: impl AsRef<str>,
) -> Result<(), ServerError> {
    let parsed: ClientMessage = serde_json::from_str(raw.as_ref())
        .map_err(|err| ServerError::InvalidMessage(err.to_string()))?;

    match parsed {
        ClientMessage::Sync {
            document: name,
            update,
        } => {
            if name != context.document_name {
                return Ok(());
            }
            let update_bytes = base64::decode(update)
                .map_err(|err| ServerError::InvalidMessage(err.to_string()))?;
            document.doc().apply_update(&update_bytes)?;

            let payload = Document::create_change_payload(
                &document,
                context.clone(),
                update_bytes.clone(),
            );
            nexus.on_change(&payload).await?;

            document.doc().broadcast(ServerMessage::Sync {
                document: context.document_name.clone(),
                update: base64::encode(update_bytes),
            });
        }
        ClientMessage::Awareness {
            document: name,
            client_id,
            state,
        } => {
            if name != context.document_name {
                return Ok(());
            }

            let payload = document.doc().set_awareness(&document, client_id, state, context.clone());
            nexus.on_awareness_update(&payload).await?;

            let states = document.doc().awareness_states();
            document.doc().broadcast(ServerMessage::Awareness {
                document: context.document_name.clone(),
                states,
            });
        }
        ClientMessage::Stateless {
            document: name,
            payload,
        } => {
            if name != context.document_name {
                return Ok(());
            }
            let stateless = Document::create_stateless_payload(
                &document,
                context.clone(),
                payload.clone(),
            );
            nexus.on_stateless(&stateless).await?;
            document.doc().broadcast(ServerMessage::Stateless {
                document: context.document_name.clone(),
                payload,
            });
        }
        ClientMessage::Token { document: _, token } => {
            let mut updated = context.clone();
            updated.token = Some(token);
            nexus.on_authenticate(&updated).await?;
        }
        ClientMessage::QueryAwareness { .. } => {
            let states = document.doc().awareness_states();
            document.doc().broadcast(ServerMessage::Awareness {
                document: context.document_name.clone(),
                states,
            });
        }
    }

    Ok(())
}

async fn normal_handle_client_message(
    nexus: &Arc<Nexus>,
    document: &DocumentHandle,
    context: &HookContext,
    message: &yrs::sync::Message,
) {
    match message {
        yrs::sync::Message::Sync(msg) => match msg {
            SyncMessage::SyncStep1(sv) => {
                let diff = document.doc().encode_state_as_update(sv);
                let reply = yrs::sync::Message::Sync(SyncMessage::SyncStep2(diff));
            }
            SyncMessage::SyncStep2(update) => {
                document.doc().apply_update(update).unwrap();
                let payload = Document::create_change_payload(
                    &document,
                    context.clone(),
                    update.clone(),
                );
                nexus.on_change(&payload);
            }
            SyncMessage::Update(update) => {
                document.doc().apply_update(update).unwrap();
            }
        },
        yrs::sync::Message::Auth(_) => {}
        yrs::sync::Message::AwarenessQuery => {}
        yrs::sync::Message::Awareness(awarenessUpdate) => {}
        yrs::sync::Message::Custom(_, _) => {}
    }
}
