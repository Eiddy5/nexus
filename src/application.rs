use crate::config::{Config, NexusSetting};
use crate::core::{ Nexus};
use crate::state::AppState;
use anyhow::Error;
use axum::Router;
use axum::extract::ws::WebSocket;
use axum::extract::{Path, State, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::routing::get;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use tracing::{info};
use crate::core::types::Extension;
use crate::extension::store::KVStoreExtension;

pub struct Application {
    listener: TcpListener,
    router: Router,
    port: u16,
}

impl Application {
    pub async fn build(config: Config, state: AppState) -> Result<Self, Error> {
        let address = format!("{}:{}", config.application.host, config.application.port);
        let listener = TcpListener::bind(address).await?;
        let port = listener.local_addr()?.port();
        let router = init_app(state).await?;
        Ok(Self {
            listener,
            router,
            port,
        })
    }

    pub async fn run(self) -> Result<(), Error> {
        info!("Server started at {}", self.listener.local_addr()?);
        axum::serve(self.listener, self.router).await?;
        Ok(())
    }
    pub fn port(&self) -> u16 {
        self.port
    }
}

pub async fn init_app(state: AppState) -> Result<Router, Error> {
    let state = Arc::new(state);
    let router = Router::new()
        .layer(TraceLayer::new_for_http())
        .route("/health", get(health_check))
        .route("/collaboration/{doc_id}", get(ws_handler))
        .with_state(state);
    Ok(router)
}
async fn health_check() -> impl IntoResponse {
    "OK"
}

async fn ws_handler(
    Path(doc_id): Path<String>,
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let nexus = state.nexus.clone();
    ws.on_upgrade(move |socket: WebSocket| async move {
        nexus.handle_connection(socket, &doc_id).await;
    })
}
pub async fn init_state(config: &Config) -> Result<AppState, Error> {
    let nexus = get_nexus(config.nexus_setting.clone());
    Ok(AppState {
        config: Arc::new(config.clone()),
        nexus: Arc::new(nexus),
    })
}

fn get_nexus(nexus_setting: NexusSetting) -> Nexus {
    info!("nexus setting: {:?}", nexus_setting);
    let extensions:Vec<Arc<dyn Extension>> = vec![
        Arc::new(KVStoreExtension::new())
    ];
    Nexus::new(&nexus_setting, extensions)
}

