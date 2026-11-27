//! AITessera (Rust) — PHP 구현과 동일한 API 계약을 갖는 Axum + SQLx 서버.
//!
//! 같은 MySQL·Redis·`.env` 를 그대로 사용하므로 PHP 서버와 나란히 띄워 성능을 비교하고,
//! 웹서버 프록시 대상만 바꿔 교체할 수 있다.

mod config;
mod domain;
mod error;
mod http;
mod infra;
mod jwt;
mod php;
mod repo;
mod response;
mod service;
mod state;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use axum::extract::Request;
use axum::ServiceExt;
use sqlx::mysql::{MySqlConnectOptions, MySqlPoolOptions};
use tower::Layer;
use tower_http::normalize_path::NormalizePathLayer;

use crate::config::Config;
use crate::infra::{Queue, RateLimiter};
use crate::state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // .env 는 현재 디렉터리부터 상위로 탐색한다 (rust/ 에서 실행해도 저장소 루트 .env 사용)
    let env_path = dotenvy::dotenv().ok();
    let base_dir = env_path
        .as_ref()
        .and_then(|p| p.parent().map(PathBuf::from))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,sqlx=warn".into()),
        )
        .init();

    let config = Config::from_env(&base_dir)?;
    error::APP_DEBUG.set(config.app_debug).ok();

    // 세션 타임존·sql_mode 는 서버 기본값을 유지한다 (PHP PDO 와 동일 — sqlx 기본 '+00:00' 강제 해제)
    let connect = MySqlConnectOptions::new()
        .host(&config.db_host)
        .port(config.db_port)
        .database(&config.db_name)
        .username(&config.db_user)
        .password(&config.db_pass)
        .charset("utf8mb4")
        .timezone(None)
        .pipes_as_concat(false)
        .statement_cache_capacity(256);
    let db = MySqlPoolOptions::new()
        .max_connections(config.db_pool_max)
        .min_connections(1)
        .acquire_timeout(Duration::from_secs(5))
        .connect_lazy_with(connect);

    let (queue, limiter) = if config.is_testing() {
        (
            Queue::Memory(Default::default()),
            RateLimiter::Memory(Default::default()),
        )
    } else {
        let client = redis::Client::open(format!(
            "redis://{}:{}/",
            config.redis_host, config.redis_port
        ))?;
        let conn = redis::aio::ConnectionManager::new(client)
            .await
            .context("Redis 연결에 실패했습니다.")?;
        (Queue::Redis(conn.clone()), RateLimiter::redis(conn))
    };

    let jwt = jwt::JwtService::new(&config)?;
    let bind = config.bind_addr.clone();
    let state = Arc::new(AppState {
        config,
        db,
        jwt,
        queue,
        limiter,
    });

    // 끝 슬래시 정규화는 라우팅 전에 적용되도록 Router 바깥에 둔다
    let app = NormalizePathLayer::trim_trailing_slash().layer(http::router(state));

    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!("AITessera (rust) listening on http://{bind}");
    axum::serve(
        listener,
        ServiceExt::<Request>::into_make_service_with_connect_info::<SocketAddr>(app),
    )
    .with_graceful_shutdown(async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await?;
    Ok(())
}
