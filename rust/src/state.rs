//! 공유 상태 (PHP DI 컨테이너에 해당) — 핸들러에 `State<Arc<AppState>>` 로 주입된다.

use std::sync::Arc;

use chrono::{NaiveDateTime, Timelike};
use sqlx::MySqlPool;

use crate::config::Config;
use crate::infra::{Queue, RateLimiter};
use crate::jwt::JwtService;

pub struct AppState {
    pub config: Config,
    pub db: MySqlPool,
    pub jwt: JwtService,
    pub queue: Queue,
    pub limiter: RateLimiter,
}

pub type SharedState = Arc<AppState>;

impl AppState {
    /// 앱 타임존 기준 현재 시각(초 단위 절삭) — PHP `format('Y-m-d H:i:s')` 로 저장하던 값과 같다
    pub fn now(&self) -> NaiveDateTime {
        chrono::Utc::now()
            .with_timezone(&self.config.app_timezone)
            .naive_local()
            .with_nanosecond(0)
            .expect("0 ns is always valid")
    }
}
