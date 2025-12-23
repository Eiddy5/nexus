use crate::config::Config;
use crate::core::{Nexus, NexusSetting};
use crate::state::AppState;
use anyhow::Error;
use axum::Router;
use axum::extract::ws::{WebSocket};
use axum::extract::{Path, State, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::routing::get;
use std::sync::Arc;
use futures_util::StreamExt;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tower_http::trace::TraceLayer;
use tracing::{debug, error, info};
use crate::core::conn::{AxumSink, AxumStream};

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
    let document = nexus.get_or_init_document(&doc_id).await;
    info!("connecting to doc: {}", doc_id);
    ws.on_upgrade(move |socket: WebSocket| async move {
        let (sink, stream) = socket.split();
        let sink = Arc::new(Mutex::new(AxumSink::from(sink)));
        let stream = AxumStream::from(stream);
        let sub = document.subscribe(sink, stream).await;
        match sub.completed().await {
            Ok(_) => debug!("👋🏻 👋🏻 👋🏻   客户端连接正常关闭"),
            Err(e) => debug!("❗❗️❗️️️客户端连接异常断开  error:{}", e),
        }
    })
}

pub async fn init_state(config: &Config) -> Result<AppState, Error> {
    let nexus = get_nexus();
    Ok(AppState {
        config: Arc::new(config.clone()),
        nexus: Arc::new(nexus),
    })
}

fn get_nexus() -> Nexus {
    let setting = NexusSetting {
        capacity: 0,
        time_to_idle: 0,
    };
    info!("nexus setting: {:?}", setting);
    Nexus::new(&setting)
}
