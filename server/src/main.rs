use anyhow::Result;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod api;
mod db;
mod matrix;

pub struct AppState {
    pub db: sqlx::PgPool,
    pub redis: redis::Client,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "firefall_server=debug,tower_http=debug".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://firefall:firefall@localhost:5432/firefall".into());
    let redis_url = std::env::var("REDIS_URL")
        .unwrap_or_else(|_| "redis://localhost:6379".into());

    tracing::info!("Connecting to database...");
    let db = PgPoolOptions::new()
        .max_connections(20)
        .connect(&database_url)
        .await?;

    tracing::info!("Running database migrations...");
    db::run_migrations(&db).await?;

    let redis = redis::Client::open(redis_url)?;
    let state = Arc::new(AppState { db, redis });

    let app = api::routes::build_router(state.clone())
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    let api_host = std::env::var("API_HOST").unwrap_or_else(|_| "0.0.0.0".into());
    let api_port = std::env::var("API_PORT").unwrap_or_else(|_| "8080".into());
    let bind_addr = format!("{api_host}:{api_port}");

    // Matrix UDP server
    let matrix_port: u16 = std::env::var("MATRIX_PORT")
        .unwrap_or_else(|_| "25000".into())
        .parse()
        .unwrap_or(25000);
    // MATRIX_PUBLIC_PORT: porta publica que o HUGG informa ao client (playit.gg tunnel)
    // Fallback: mesma porta do bind (para uso local sem tunnel)
    let matrix_game_port: u16 = std::env::var("MATRIX_PUBLIC_PORT")
        .unwrap_or_else(|_| matrix_port.to_string())
        .parse()
        .unwrap_or(matrix_port);
    tokio::spawn(matrix::server::start(matrix_port, matrix_game_port));

    tracing::info!("Firefall server listening on {bind_addr} (HTTP)");
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
