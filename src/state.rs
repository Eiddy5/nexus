use crate::config::Config;
use crate::core::Nexus;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub nexus: Arc<Nexus>,
}
