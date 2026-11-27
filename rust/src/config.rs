//! 환경설정 — PHP `App\Support\Config::fromEnv()` 와 같은 키·기본값·검증 규칙을 따른다.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use chrono_tz::Tz;

/// JWT 서명 알고리즘 (PHP `JwtAlgorithm` enum)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JwtAlgorithm {
    Hs256,
    Rs256,
}

impl JwtAlgorithm {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hs256 => "HS256",
            Self::Rs256 => "RS256",
        }
    }

    pub fn is_asymmetric(self) -> bool {
        self == Self::Rs256
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub app_env: String,
    pub app_debug: bool,
    pub db_host: String,
    pub db_port: u16,
    pub db_name: String,
    pub db_user: String,
    pub db_pass: String,
    pub db_pool_max: u32,
    pub redis_host: String,
    pub redis_port: u16,
    pub jwt_secret: String,
    pub jwt_algo: JwtAlgorithm,
    pub jwt_private_key_path: PathBuf,
    pub jwt_public_key_path: PathBuf,
    pub jwt_private_key_passphrase: String,
    pub jwt_access_ttl: i64,
    pub jwt_refresh_ttl: i64,
    pub email_verify_ttl: i64,
    pub rate_limit_auth: i64,
    pub rate_limit_api: i64,
    /// PHP `date.timezone` 과 동일해야 DB DATETIME 문자열이 일치한다 (기본 UTC)
    pub app_timezone: Tz,
    pub bind_addr: String,
}

impl Config {
    /// `.env` 를 찾은 디렉터리(`base_dir`) 기준으로 상대 키 경로를 해석한다.
    pub fn from_env(base_dir: &Path) -> Result<Self> {
        let jwt_algo = match env_str("JWT_ALGO", "").as_str() {
            "" => JwtAlgorithm::Hs256,
            "HS256" => JwtAlgorithm::Hs256,
            "RS256" => JwtAlgorithm::Rs256,
            other => bail!("지원하지 않는 JWT_ALGO 값입니다: {other} (허용: HS256, RS256)"),
        };
        let jwt_secret = env_str("JWT_SECRET", "");
        let private_raw = env_str("JWT_PRIVATE_KEY_PATH", "");
        let public_raw = env_str("JWT_PUBLIC_KEY_PATH", "");

        // 알고리즘별 필수 시크릿을 부팅 시점에 fail-fast 로 검증한다.
        if jwt_algo.is_asymmetric() {
            if private_raw.is_empty() || public_raw.is_empty() {
                bail!("JWT_ALGO=RS256 에는 JWT_PRIVATE_KEY_PATH·JWT_PUBLIC_KEY_PATH 가 모두 필요합니다.");
            }
        } else if jwt_secret.is_empty() {
            bail!("JWT_SECRET 환경변수가 설정되지 않았습니다.");
        }

        let resolve = |raw: &str| -> PathBuf {
            if raw.is_empty() {
                return PathBuf::new();
            }
            let p = PathBuf::from(raw);
            if p.is_absolute() {
                p
            } else {
                base_dir.join(p)
            }
        };
        let jwt_private_key_path = resolve(&private_raw);
        let jwt_public_key_path = resolve(&public_raw);
        if jwt_algo.is_asymmetric() {
            for (label, path) in [
                ("개인키", &jwt_private_key_path),
                ("공개키", &jwt_public_key_path),
            ] {
                std::fs::metadata(path).with_context(|| {
                    format!("JWT {label} 파일을 읽을 수 없습니다: {}", path.display())
                })?;
            }
        }

        let tz_name = env_str("APP_TIMEZONE", "UTC");
        let app_timezone: Tz = tz_name
            .parse()
            .map_err(|_| anyhow::anyhow!("APP_TIMEZONE 값이 올바르지 않습니다: {tz_name}"))?;

        Ok(Self {
            app_env: env_str("APP_ENV", "production"),
            app_debug: env_bool("APP_DEBUG", false),
            db_host: env_str("DB_HOST", "127.0.0.1"),
            db_port: env_int("DB_PORT", 3306) as u16,
            db_name: env_str("DB_NAME", "aitessera"),
            db_user: env_str("DB_USER", "root"),
            db_pass: env_str("DB_PASS", ""),
            db_pool_max: env_int("DB_POOL_MAX", 32) as u32,
            redis_host: env_str("REDIS_HOST", "127.0.0.1"),
            redis_port: env_int("REDIS_PORT", 6379) as u16,
            jwt_secret,
            jwt_algo,
            jwt_private_key_path,
            jwt_public_key_path,
            jwt_private_key_passphrase: env_str("JWT_PRIVATE_KEY_PASSPHRASE", ""),
            jwt_access_ttl: env_int("JWT_ACCESS_TTL", 900),
            jwt_refresh_ttl: env_int("JWT_REFRESH_TTL", 1_209_600),
            email_verify_ttl: env_int("EMAIL_VERIFY_TTL", 86_400),
            rate_limit_auth: env_int("RATE_LIMIT_AUTH", 10),
            rate_limit_api: env_int("RATE_LIMIT_API", 120),
            app_timezone,
            bind_addr: env_str("RUST_BIND_ADDR", "127.0.0.1:8306"),
        })
    }

    pub fn is_testing(&self) -> bool {
        self.app_env == "testing"
    }
}

/// PHP `Config::str()` — 비어 있으면 기본값
fn env_str(key: &str, default: &str) -> String {
    match std::env::var(key) {
        Ok(v) if !v.is_empty() => v,
        _ => default.to_string(),
    }
}

/// PHP `Config::int()` — is_numeric 이면 (int) 캐스트, 아니면 기본값.
/// `.env` 의 행 끝 주석(`10    # ...`)은 dotenvy 가 제거한다.
fn env_int(key: &str, default: i64) -> i64 {
    std::env::var(key)
        .ok()
        .and_then(|v| crate::php::numeric_to_int(&serde_json::Value::String(v)))
        .unwrap_or(default)
}

/// PHP `Config::bool()`
fn env_bool(key: &str, default: bool) -> bool {
    match std::env::var(key) {
        Ok(v) => matches!(v.to_lowercase().as_str(), "1" | "true" | "on" | "yes"),
        Err(_) => default,
    }
}
