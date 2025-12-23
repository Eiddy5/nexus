mod application;
mod backup;
mod config;
mod core;
mod errors;
mod state;
mod utils;
mod extension;

use crate::application::{Application, init_state};
use crate::config::get_configuration;
use crate::utils::env_util::get_env_var;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

pub type AwarenessRef = Arc<RwLock<yrs::sync::Awareness>>;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let log_level = get_env_var("LOG_LEVEL", "info");
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::new(log_level))
        .init();
    let config =
        get_configuration().map_err(|e| anyhow::anyhow!("Failed to read configuration: {}", e))?;
    let state = init_state(&config).await?;
    let application = Application::build(config, state).await?;
    application.run().await?;
    Ok(())
}
