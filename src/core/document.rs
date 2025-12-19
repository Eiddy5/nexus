use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::sync::broadcast::{Sender, channel};

pub type AwarenessRef = Arc<RwLock<yrs::sync::Awareness>>;

pub struct Document {
    awareness: AwarenessRef,
    sender: Sender<Vec<u8>>,
}

unsafe impl Send for Document {}
unsafe impl Sync for Document {}

impl Document {
    pub fn new(awareness: AwarenessRef) -> Self {
        let (sender, receiver) = channel(1024);

        Self { awareness, sender }
    }
}
