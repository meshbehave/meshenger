use std::convert::Infallible;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, Sse};
use axum::response::Json;
use axum::routing::get;
use axum::Router;
use futures_util::stream::Stream;
use serde::{Deserialize, Serialize};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};

use crate::config::Config;
use crate::db::{Db, MqttFilter};

fn to_json<T: Serialize>(value: T) -> Result<Json<serde_json::Value>, StatusCode> {
    serde_json::to_value(value).map(Json).map_err(|e| {
        log::error!("JSON serialization error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

#[derive(Clone)]
struct AppState {
    db: Arc<Db>,
    config: Arc<Config>,
    queue_depth: Arc<AtomicUsize>,
    local_node_id: Arc<std::sync::atomic::AtomicU32>,
    sse_tx: tokio::sync::broadcast::Sender<()>,
}

fn default_mqtt() -> String {
    "all".to_string()
}

#[derive(Deserialize)]
struct HoursParam {
    #[serde(default = "default_hours")]
    hours: u32,
    #[serde(default = "default_mqtt")]
    mqtt: String,
}

fn default_hours() -> u32 {
    24
}

#[derive(Deserialize)]
struct PacketThroughputParam {
    #[serde(default = "default_hours")]
    hours: u32,
    #[serde(default = "default_mqtt")]
    mqtt: String,
    #[serde(default)]
    types: Option<String>,
}

#[derive(Serialize)]
struct QueueResponse {
    depth: usize,
}

pub struct Dashboard {
    config: Arc<Config>,
    db: Arc<Db>,
    queue_depth: Arc<AtomicUsize>,
    local_node_id: Arc<std::sync::atomic::AtomicU32>,
    sse_tx: tokio::sync::broadcast::Sender<()>,
}

impl Dashboard {
    pub fn new(
        config: Arc<Config>,
        db: Arc<Db>,
        queue_depth: Arc<AtomicUsize>,
        local_node_id: Arc<std::sync::atomic::AtomicU32>,
        sse_tx: tokio::sync::broadcast::Sender<()>,
    ) -> Self {
        Self {
            config,
            db,
            queue_depth,
            local_node_id,
            sse_tx,
        }
    }

    pub async fn run(self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let bind = &self.config.dashboard.bind_address;
        log::info!("Starting dashboard on {}", bind);

        let state = AppState {
            db: self.db,
            config: self.config.clone(),
            queue_depth: self.queue_depth,
            local_node_id: self.local_node_id,
            sse_tx: self.sse_tx,
        };

        let api_routes = Router::new()
            .route("/api/overview", get(handle_overview))
            .route("/api/nodes", get(handle_nodes))
            .route("/api/throughput", get(handle_throughput))
            .route("/api/packet-throughput", get(handle_packet_throughput))
            .route("/api/rssi", get(handle_rssi))
            .route("/api/snr", get(handle_snr))
            .route("/api/hops", get(handle_hops))
            .route(
                "/api/traceroute-requesters",
                get(handle_traceroute_requesters),
            )
            .route("/api/traceroute-events", get(handle_traceroute_events))
            .route(
                "/api/traceroute-destinations",
                get(handle_traceroute_destinations),
            )
            .route("/api/hops-to-me", get(handle_hops_to_me))
            .route("/api/traceroute-sessions", get(handle_traceroute_sessions))
            .route(
                "/api/traceroute-sessions/{id}",
                get(handle_traceroute_session_detail),
            )
            .route("/api/positions", get(handle_positions))
            .route("/api/queue", get(handle_queue))
            .route("/api/events", get(handle_sse));

        // Serve static files from web/dist/ if the directory exists (prod mode)
        let app = if std::path::Path::new("web/dist/index.html").exists() {
            let serve_dir =
                ServeDir::new("web/dist").fallback(ServeFile::new("web/dist/index.html"));
            api_routes
                .fallback_service(serve_dir)
                .layer(CorsLayer::permissive())
                .with_state(state)
        } else {
            api_routes.layer(CorsLayer::permissive()).with_state(state)
        };

        let listener = tokio::net::TcpListener::bind(bind).await?;
        log::info!("Dashboard listening on {}", bind);
        axum::serve(listener, app).await?;
        Ok(())
    }
}

async fn handle_overview(
    State(state): State<AppState>,
    Query(params): Query<HoursParam>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let filter = MqttFilter::from_str(&params.mqtt);
    let overview = state
        .db
        .dashboard_overview(params.hours, filter, &state.config.bot.name)
        .map_err(|e| {
            log::error!("Dashboard overview error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    to_json(overview)
}

async fn handle_nodes(
    State(state): State<AppState>,
    Query(params): Query<HoursParam>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let filter = MqttFilter::from_str(&params.mqtt);
    let nodes = state
        .db
        .dashboard_nodes(params.hours, filter)
        .map_err(|e| {
            log::error!("Dashboard nodes error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    to_json(nodes)
}

async fn handle_throughput(
    State(state): State<AppState>,
    Query(params): Query<HoursParam>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let filter = MqttFilter::from_str(&params.mqtt);
    let buckets = state
        .db
        .dashboard_throughput(params.hours, filter)
        .map_err(|e| {
            log::error!("Dashboard throughput error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    to_json(buckets)
}

async fn handle_packet_throughput(
    State(state): State<AppState>,
    Query(params): Query<PacketThroughputParam>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let filter = MqttFilter::from_str(&params.mqtt);
    let packet_types: Option<Vec<String>> = params.types.map(|t| {
        t.split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    });
    let buckets = state
        .db
        .dashboard_packet_throughput(params.hours, filter, packet_types.as_deref())
        .map_err(|e| {
            log::error!("Dashboard packet throughput error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    to_json(buckets)
}

async fn handle_rssi(
    State(state): State<AppState>,
    Query(params): Query<HoursParam>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let filter = MqttFilter::from_str(&params.mqtt);
    let buckets = state.db.dashboard_rssi(params.hours, filter).map_err(|e| {
        log::error!("Dashboard RSSI error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    to_json(buckets)
}

async fn handle_snr(
    State(state): State<AppState>,
    Query(params): Query<HoursParam>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let filter = MqttFilter::from_str(&params.mqtt);
    let buckets = state.db.dashboard_snr(params.hours, filter).map_err(|e| {
        log::error!("Dashboard SNR error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    to_json(buckets)
}

async fn handle_hops(
    State(state): State<AppState>,
    Query(params): Query<HoursParam>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let filter = MqttFilter::from_str(&params.mqtt);
    let buckets = state.db.dashboard_hops(params.hours, filter).map_err(|e| {
        log::error!("Dashboard hops error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    to_json(buckets)
}

async fn handle_positions(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let positions = state.db.dashboard_positions().map_err(|e| {
        log::error!("Dashboard positions error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    to_json(positions)
}

async fn handle_traceroute_requesters(
    State(state): State<AppState>,
    Query(params): Query<HoursParam>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let local_node_id = state.local_node_id.load(Ordering::Relaxed);
    if local_node_id == 0 {
        return to_json(Vec::<serde_json::Value>::new());
    }

    let filter = MqttFilter::from_str(&params.mqtt);
    let rows = state
        .db
        .dashboard_traceroute_requesters(local_node_id, params.hours, filter)
        .map_err(|e| {
            log::error!("Dashboard traceroute requesters error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    to_json(rows)
}

async fn handle_traceroute_events(
    State(state): State<AppState>,
    Query(params): Query<HoursParam>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let filter = MqttFilter::from_str(&params.mqtt);
    let rows = state
        .db
        .dashboard_traceroute_events(params.hours, filter, 200)
        .map_err(|e| {
            log::error!("Dashboard traceroute events error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    to_json(rows)
}

async fn handle_traceroute_destinations(
    State(state): State<AppState>,
    Query(params): Query<HoursParam>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let filter = MqttFilter::from_str(&params.mqtt);
    let rows = state
        .db
        .dashboard_traceroute_destinations(params.hours, filter)
        .map_err(|e| {
            log::error!("Dashboard traceroute destinations error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    to_json(rows)
}

async fn handle_hops_to_me(
    State(state): State<AppState>,
    Query(params): Query<HoursParam>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let local_node_id = state.local_node_id.load(Ordering::Relaxed);
    if local_node_id == 0 {
        return to_json(Vec::<serde_json::Value>::new());
    }

    let filter = MqttFilter::from_str(&params.mqtt);
    let rows = state
        .db
        .dashboard_hops_to_me(local_node_id, params.hours, filter)
        .map_err(|e| {
            log::error!("Dashboard hops-to-me error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    to_json(rows)
}

async fn handle_traceroute_sessions(
    State(state): State<AppState>,
    Query(params): Query<HoursParam>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let filter = MqttFilter::from_str(&params.mqtt);
    let rows = state
        .db
        .dashboard_traceroute_sessions(params.hours, filter, 200)
        .map_err(|e| {
            log::error!("Dashboard traceroute sessions error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    to_json(rows)
}

async fn handle_traceroute_session_detail(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let row = state
        .db
        .dashboard_traceroute_session_detail(id)
        .map_err(|e| {
            log::error!("Dashboard traceroute session detail error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    to_json(row)
}

async fn handle_queue(State(state): State<AppState>) -> Json<QueueResponse> {
    Json(QueueResponse {
        depth: state.queue_depth.load(Ordering::Relaxed),
    })
}

async fn handle_sse(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.sse_tx.subscribe();
    let stream = BroadcastStream::new(rx).map(|_| Ok(Event::default().event("refresh").data("")));
    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(30))
            .text("ping"),
    )
}
