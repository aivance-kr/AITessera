//! 인프라 어댑터 — 큐(Redis 리스트)·레이트 리밋(sliding window)·비밀번호 해시.
//!
//! 큐 포맷은 PHP `RedisQueue` 와 같다(LPUSH 채널 + JSON) — 기존 PHP 워커(mail:work·log:work·
//! security:scan)가 Rust 서버가 적재한 메시지를 그대로 소비한다.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

use redis::aio::ConnectionManager;
use serde_json::Value;

use crate::error::{AppError, AppResult};

// ───────────────────────────── 큐 ─────────────────────────────

pub enum Queue {
    Redis(ConnectionManager),
    /// APP_ENV=testing — PHP InMemoryQueue 와 동일하게 Redis 없이 동작
    Memory(Mutex<HashMap<String, VecDeque<String>>>),
}

impl Queue {
    pub async fn push(&self, channel: &str, payload: &Value) -> AppResult<()> {
        let body = php_json_encode(payload);
        match self {
            Self::Redis(conn) => {
                let mut conn = conn.clone();
                redis::cmd("LPUSH")
                    .arg(channel)
                    .arg(body)
                    .query_async::<i64>(&mut conn)
                    .await?;
            }
            Self::Memory(store) => {
                store
                    .lock()
                    .map_err(|_| AppError::Internal("queue lock poisoned".into()))?
                    .entry(channel.to_string())
                    .or_default()
                    .push_front(body);
            }
        }
        Ok(())
    }
}

/// PHP `json_encode($v)` 기본 플래그 재현 — 비ASCII 는 `\uXXXX`, `/` 는 `\/` 로 이스케이프
pub fn php_json_encode(v: &Value) -> String {
    let raw = serde_json::to_string(&crate::php::php_normalize(v.clone())).unwrap_or_else(|_| "null".into());
    let mut out = String::with_capacity(raw.len() + 16);
    for ch in raw.chars() {
        match ch {
            '/' => out.push_str("\\/"),
            c if (c as u32) < 0x80 => out.push(c),
            c => {
                let mut buf = [0u16; 2];
                for unit in c.encode_utf16(&mut buf) {
                    out.push_str(&format!("\\u{unit:04x}"));
                }
            }
        }
    }
    out
}

// ───────────────────────────── 레이트 리밋 ─────────────────────────────

/// symfony/rate-limiter `SlidingWindowLimiter` 알고리즘을 Redis Lua 로 원자 실행한다.
/// 거절된 요청도 hit 로 누적되는 점까지 원본과 같다.
/// 반환: {accepted(0|1), wait_seconds(string float)}
const SLIDING_WINDOW_LUA: &str = r#"
local key = KEYS[1]
local now = tonumber(ARGV[1])
local interval = tonumber(ARGV[2])
local limit = tonumber(ARGV[3])
local data = redis.call('HMGET', key, 'end', 'hits', 'prev')
local wend, hits, prev
if not data[1] then
  wend = now + interval; hits = 0; prev = 0
else
  wend = tonumber(data[1]); hits = tonumber(data[2]); prev = tonumber(data[3])
  if now > wend then
    local next_end = wend + interval
    if now < next_end then prev = hits; wend = next_end else prev = 0; wend = now + interval end
    hits = 0
  end
end
local function hit_count()
  local start = wend - interval
  local pct = math.min((now - start) / interval, 1)
  return math.floor(prev * (1 - pct) + hits)
end
local available = math.max(0, limit - hit_count())
local accepted = 0
local wait = 0
if available >= 1 then
  accepted = 1
else
  local remaining = limit - hit_count()
  local start = wend - interval
  local passed = now - start
  local wpassed = math.min(passed / interval, 1)
  local releasable = math.max(1, limit - math.floor(prev * (1 - wpassed)))
  local remaining_window = interval - passed
  local needed = 1 - remaining
  if releasable >= needed then
    wait = needed * (remaining_window / math.max(1, releasable))
  else
    wait = (wend - now) + (needed - releasable) * (interval / limit)
  end
end
hits = hits + 1
redis.call('HSET', key, 'end', tostring(wend), 'hits', hits, 'prev', prev)
redis.call('EXPIRE', key, math.ceil(interval * 2) + 1)
return {accepted, tostring(wait)}
"#;

pub enum RateLimiter {
    Redis { conn: ConnectionManager, script: Box<redis::Script> },
    Memory(Mutex<HashMap<String, Window>>),
}

pub struct Window {
    end: f64,
    hits: i64,
    prev: i64,
}

pub const RATE_INTERVAL: f64 = 60.0;

impl RateLimiter {
    pub fn redis(conn: ConnectionManager) -> Self {
        Self::Redis { conn, script: Box::new(redis::Script::new(SLIDING_WINDOW_LUA)) }
    }

    /// 토큰 1개 소비 — 거절이면 Some(Retry-After 초)
    pub async fn consume(&self, id: &str, ip: &str, limit: i64) -> AppResult<Option<i64>> {
        let now = now_f64();
        let (accepted, wait) = match self {
            Self::Redis { conn, script } => {
                let mut conn = conn.clone();
                let key = format!("ratelimit:rust:{id}:{ip}");
                let (accepted, wait): (i64, String) = script
                    .key(key)
                    .arg(format!("{now:.6}"))
                    .arg(RATE_INTERVAL)
                    .arg(limit)
                    .invoke_async(&mut conn)
                    .await?;
                (accepted == 1, wait.parse::<f64>().unwrap_or(0.0))
            }
            Self::Memory(store) => {
                let mut map = store.lock().map_err(|_| AppError::Internal("limiter lock poisoned".into()))?;
                let w = map.entry(format!("{id}:{ip}")).or_insert(Window { end: now + RATE_INTERVAL, hits: 0, prev: 0 });
                memory_consume(w, now, limit)
            }
        };
        if accepted {
            return Ok(None);
        }
        // PHP: max(1, floor(now + wait) - floor(now))
        let retry_after = ((now + wait).floor() as i64 - now.floor() as i64).max(1);
        Ok(Some(retry_after))
    }
}

fn memory_consume(w: &mut Window, now: f64, limit: i64) -> (bool, f64) {
    if now > w.end {
        let next_end = w.end + RATE_INTERVAL;
        if now < next_end {
            w.prev = w.hits;
            w.end = next_end;
        } else {
            w.prev = 0;
            w.end = now + RATE_INTERVAL;
        }
        w.hits = 0;
    }
    let hit_count = |w: &Window| {
        let pct = ((now - (w.end - RATE_INTERVAL)) / RATE_INTERVAL).min(1.0);
        (w.prev as f64 * (1.0 - pct) + w.hits as f64).floor() as i64
    };
    let available = (limit - hit_count(w)).max(0);
    let mut wait = 0.0;
    let accepted = available >= 1;
    if !accepted {
        let remaining = limit - hit_count(w);
        let passed = now - (w.end - RATE_INTERVAL);
        let wpassed = (passed / RATE_INTERVAL).min(1.0);
        let releasable = (limit as f64 - (w.prev as f64 * (1.0 - wpassed)).floor()).max(1.0);
        let needed = (1 - remaining) as f64;
        wait = if releasable >= needed {
            needed * ((RATE_INTERVAL - passed) / releasable.max(1.0))
        } else {
            (w.end - now) + (needed - releasable) * (RATE_INTERVAL / limit as f64)
        };
    }
    w.hits += 1;
    (accepted, wait)
}

fn now_f64() -> f64 {
    let now = chrono::Utc::now();
    now.timestamp() as f64 + f64::from(now.timestamp_subsec_micros()) / 1_000_000.0
}

// ───────────────────────────── 비밀번호 해시 ─────────────────────────────

/// `password_hash($pw, PASSWORD_ARGON2ID)` — PHP 기본 파라미터(m=65536, t=4, p=1)와 동일한 PHC 문자열.
/// CPU 집약 연산이므로 blocking 풀에서 실행한다.
pub async fn password_hash(password: String) -> AppResult<String> {
    tokio::task::spawn_blocking(move || {
        use argon2::password_hash::{rand_core::OsRng, PasswordHasher, SaltString};
        let params = argon2::Params::new(65_536, 4, 1, Some(32)).map_err(|e| AppError::Internal(e.to_string()))?;
        let hasher = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
        let salt = SaltString::generate(&mut OsRng);
        hasher
            .hash_password(password.as_bytes(), &salt)
            .map(|h| h.to_string())
            .map_err(|e| AppError::Internal(e.to_string()))
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

/// `password_verify()` — argon2id/argon2i(PHC) 및 bcrypt(`$2y$`) 해시 지원
pub async fn password_verify(password: String, hash: String) -> AppResult<bool> {
    tokio::task::spawn_blocking(move || {
        if hash.starts_with("$argon2") {
            use argon2::password_hash::{PasswordHash, PasswordVerifier};
            return PasswordHash::new(&hash)
                .ok()
                .is_some_and(|parsed| argon2::Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok());
        }
        if hash.starts_with("$2") {
            return bcrypt::verify(password.as_bytes(), &hash).unwrap_or(false);
        }
        false
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn php_json_encode_matches_php_defaults() {
        assert_eq!(php_json_encode(&json!({"a": "한/b", "c": {}})), "{\"a\":\"\\ud55c\\/b\",\"c\":[]}");
    }

    #[test]
    fn memory_window_rejects_after_limit() {
        let mut w = Window { end: 1000.0 + RATE_INTERVAL, hits: 0, prev: 0 };
        for _ in 0..3 {
            assert!(memory_consume(&mut w, 1000.5, 3).0);
        }
        assert!(!memory_consume(&mut w, 1000.5, 3).0);
    }
}
