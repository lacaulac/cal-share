use axum::{
    routing::{get, post},
    Router,
};
use std::net::SocketAddr;
use std::path::PathBuf;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod auth;
mod handlers;
mod state;

use state::AppState;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load optional .env file
    let _ = dotenvy::dotenv();

    // Initialize structured tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "axum_ical_service=debug,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Configuration
    let jwt_secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| {
        tracing::warn!("JWT_SECRET not set; falling back to default development secret. DO NOT USE IN PRODUCTION.");
        "default-super-secret-jwt-key-change-in-production".to_string()
    });

    let storage_dir = PathBuf::from(
        std::env::var("CALENDAR_STORAGE_DIR").unwrap_or_else(|_| "./data/calendars".to_string()),
    );

    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(8080);

    // Ensure calendar storage directory exists
    tokio::fs::create_dir_all(&storage_dir).await?;
    tracing::info!(
        "iCalendar storage directory initialized at: {}",
        storage_dir.display()
    );

    // Application state
    let state = AppState::new(jwt_secret, storage_dir);

    // Configure CORS for web and client integrations
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Build the Axum router
    let app = Router::new()
        .route("/create_user", post(handlers::create_user))
        .route("/upload", post(handlers::upload_calendar))
        .route("/calendar/{identifier}", get(handlers::get_calendar))
        // Backwards compatibility with colon path syntax if accessed via client
        // .route("/calendar/:identifier", get(handlers::get_calendar))
        .route("/health", get(handlers::health_check))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", host, port).parse()?;
    tracing::info!(
        "Starting Axum iCalendar API server listening on http://{}",
        addr
    );

    let listener = tokio::net::TcpListener::bind(addr).await?;

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("Shutdown signal received, gracefully closing connections...");
}
