pub(crate) use crate::AwarenessRef;
use crate::core::connection::Connection;
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::broadcast::{Sender};
use tokio::sync::{Mutex, broadcast};
use tokio::task::JoinHandle;
use tracing::{error};
use yrs::encoding::write::Write;
use yrs::sync::protocol::{MSG_SYNC, MSG_SYNC_UPDATE};
use yrs::sync::{DefaultProtocol, Error, Protocol, SyncMessage, YMessage};
use yrs::updates::decoder::Decode;
use yrs::updates::encoder::{Encode, Encoder, EncoderV1};
use yrs::{ReadTxn, Transact, Update};

pub struct Document {
    pub doc_id:String,
    observer_subs: Arc<Mutex<Vec<yrs::Subscription>>>,
    awareness_ref: AwarenessRef,
    sender: Sender<Vec<u8>>,
    awareness_updater: JoinHandle<()>,
}

impl Document {
    pub async fn new(doc_id: String, awareness: AwarenessRef) -> Self {
        let (sender, _receiver) = broadcast::channel(1024);
        let awareness_c = Arc::downgrade(&awareness);
        let lock = awareness.read().await;
        let sink = sender.clone();
        let doc_sub = {
            lock.doc()
                .observe_update_v1(move |_txn, u| {
                    let mut encoder = EncoderV1::new();
                    encoder.write_var(MSG_SYNC);
                    encoder.write_var(MSG_SYNC_UPDATE);
                    encoder.write_buf(&u.update);
                    let msg = encoder.to_vec();
                    if let Err(_e) = sink.send(msg) {
                        error!(
                            "failed to send sync message,current broadcast group is being closed"
                        )
                    }
                })
                .unwrap()
        };
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let sink = sender.clone();
        let awareness_sub = lock.on_update(move |_, e, _| {
            let added = e.added();
            let updated = e.updated();
            let removed = e.removed();
            let mut changed = Vec::with_capacity(added.len() + updated.len() + removed.len());
            changed.extend_from_slice(added);
            changed.extend_from_slice(updated);
            changed.extend_from_slice(removed);

            if let Err(_) = tx.send(changed) {
                error!("failed to send awareness update");
            }
        });
        drop(lock);
        let awareness_updater = tokio::task::spawn(async move {
            while let Some(changed_clients) = rx.recv().await {
                if let Some(awareness) = awareness_c.upgrade() {
                    let awareness = awareness.read().await;
                    match awareness.update_with_clients(changed_clients) {
                        Ok(update) => {
                            if let Err(_) = sink.send(YMessage::Awareness(update).encode_v1()) {
                                error!("couldn't broadcast awareness update");
                            }
                        }
                        Err(e) => {
                            error!("error while computing awareness update: {}", e)
                        }
                    }
                } else {
                    return;
                }
            }
        });
        Document {
            doc_id,
            awareness_ref: awareness,
            awareness_updater,
            sender,
            observer_subs: Arc::new(Mutex::new(vec![awareness_sub, doc_sub])),
        }
    }

    pub fn awareness(&self) -> &AwarenessRef {
        &self.awareness_ref
    }


    pub async fn subscribe<Sink, Stream, E>(
        &self,
        sink: Arc<Mutex<Sink>>,
        stream: Stream,
    ) -> Connection
    where
        Sink: SinkExt<Vec<u8>> + Send + Sync + Unpin + 'static,
        Stream: StreamExt<Item = Result<Vec<u8>, E>> + Send + Sync + Unpin + 'static,
        <Sink as futures_util::Sink<Vec<u8>>>::Error: std::error::Error + Send + Sync,
        E: std::error::Error + Send + Sync + 'static,
    {
        self.subscribe_with(sink, stream, DefaultProtocol).await
    }

    pub async fn subscribe_with<Sink, Stream, E, P>(
        &self,
        sink: Arc<Mutex<Sink>>,
        mut stream: Stream,
        protocol: P,
    ) -> Connection
    where
        Sink: SinkExt<Vec<u8>> + Send + Sync + Unpin + 'static,
        Stream: StreamExt<Item = Result<Vec<u8>, E>> + Send + Sync + Unpin + 'static,
        <Sink as futures_util::Sink<Vec<u8>>>::Error: std::error::Error + Send + Sync,
        E: std::error::Error + Send + Sync + 'static,
        P: Protocol + Send + Sync + 'static,
    {
        let sink_task = {
            let sink = sink.clone();
            let mut receiver = self.sender.subscribe();
            tokio::spawn(async move {
                while let Ok(update) = receiver.recv().await {
                    let mut sink = sink.lock().await;
                    let vec = update.clone();
                    if let Err(e) = sink.send(update).await {
                        error!("broadcast failed to sent sync message {:?}", vec);
                        return Err(Error::Other(Box::new(e)));
                    }
                }
                Ok(())
            })
        };
        let stream_task = {
            let sink = sink.clone();
            let awareness = self.awareness().clone();
            tokio::spawn(async move {
                while let Some(msg) = stream.next().await {
                    let msg = YMessage::decode_v1(&msg.map_err(|e| Error::Other(Box::new(e)))?)?;
                    if let Some(reply) = Self::handle_msg(&protocol, &awareness, msg).await? {
                        let mut sink = sink.lock().await;
                        sink.send(reply.encode_v1())
                            .await
                            .map_err(|e| Error::Other(Box::new(e)))?;
                    }
                }
                Ok(())
            })
        };
        let (sv, awareness) = {
            let sv = self
                .awareness()
                .read()
                .await
                .doc()
                .transact()
                .state_vector();
            let awareness_update = self.awareness().read().await.update().unwrap();
            (sv, awareness_update)
        };
        {
            let mut sink = sink.lock().await;
            sink.send(YMessage::Sync(SyncMessage::SyncStep1(sv)).encode_v1())
                .await
                .map_err(|e| Error::Other(Box::new(e)))
                .unwrap();
            sink.send(YMessage::Awareness(awareness).encode_v1())
                .await
                .map_err(|e| Error::Other(Box::new(e)))
                .unwrap();
        }
        Connection::new(sink_task, stream_task)
    }

    async fn handle_msg<P: Protocol>(
        protocol: &P,
        awareness: &AwarenessRef,
        msg: YMessage,
    ) -> Result<Option<YMessage>, Error> {
        match msg {
            YMessage::Sync(msg) => match msg {
                SyncMessage::SyncStep1(state_vector) => {
                    let awareness = awareness.read().await;
                    let result = protocol.handle_sync_step1(&*awareness, state_vector);
                    result
                }
                SyncMessage::SyncStep2(update) => {
                    Self::handle_update(awareness, protocol, update).await;
                    Ok(None)
                }
                SyncMessage::Update(update) => {
                    Self::handle_update(awareness, protocol, update).await;
                    Ok(None)
                }
            },
            YMessage::Auth(deny_reason) => {
                let awareness = awareness.read().await;
                protocol.handle_auth(&*awareness, deny_reason)
            }
            YMessage::AwarenessQuery => {
                let awareness = awareness.read().await;
                protocol.handle_awareness_query(&*awareness)
            }
            YMessage::Awareness(update) => {
                let mut awareness = awareness.write().await;
                protocol.handle_awareness_update(&mut *awareness, update)
            }
            YMessage::Custom(tag, data) => {
                let mut awareness = awareness.write().await;
                protocol.missing_handle(&mut *awareness, tag, data)
            }
        }
    }

    pub async fn on_update<F>(&self, callback: F)
    where
        F: Fn(Vec<u8>) + Send  + 'static + Sync,
    {
        let lock = self.awareness().read().await;
        let sub = {
            lock.doc()
                .observe_update_v1(move |_txn, u| {
                    callback(u.update.clone())
                })
                .unwrap()
        };
        drop(lock);
        self.observer_subs.lock().await.push(sub);
    }
    async fn handle_update<P: Protocol>(awareness: &AwarenessRef, protocol: &P, update: Vec<u8>) {
        let mut awareness = awareness.write().await;
        match Update::decode_v1(&update) {
            Ok(update) => {
                let _ = protocol
                    .handle_sync_step2(&mut *awareness, update)
                    .map_err(|e| error!("handle_sync_step2 failed {}", e));
            }
            Err(e) => {
                error!("decode_update failed {}", e)
            }
        }
    }
}

impl Drop for Document {
    fn drop(&mut self) {
        self.awareness_updater.abort();
    }
}
