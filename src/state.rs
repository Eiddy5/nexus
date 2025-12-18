use std::sync::Arc;
use crate::config::Config;
use crate::core::Nexus;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    // pub nexus: Arc<Nexus>
}
