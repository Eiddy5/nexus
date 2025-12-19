use crate::config::Config;
use crate::core::{Nexus, default_setting};
use crate::state::AppState;
use anyhow::Error;
use axum::Router;
use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path, State, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::routing::get;
use futures::{SinkExt, StreamExt};
use mpsc::channel;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tower_http::trace::TraceLayer;
use tracing::{debug, info};

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
    debug!("{:} 正在建立连接... ", doc_id);
    let nexus = state.nexus.clone();
    ws.on_upgrade(move |socket: WebSocket| async move {
        nexus.handle_connection(socket).await
    })
}

pub async fn init_state(config: &Config) -> Result<AppState, Error> {
    let nexus = get_nexus();
    Ok(AppState {
        config: Arc::new(config.clone()),
        nexus: Arc::new(nexus)
    })
}

fn get_nexus() -> Nexus {
    let setting = default_setting();
    info!("nexus setting: {:?}", setting);
    Nexus::new(Some(setting),vec![])
}
