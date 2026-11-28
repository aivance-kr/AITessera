//! 도메인 예외 → 표준 에러 응답 (PHP `App\Exception\*` + `ErrorHandlerMiddleware`).

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

pub use crate::response::error_response;

/// PHP 의 예외(throw) 경로로 만들어진 응답 표시.
/// PHP 에서는 예외가 CorsMiddleware 를 그대로 통과해 ErrorHandler 에서 응답이 만들어지므로
/// CORS 헤더가 붙지 않는다 — 이 마커가 있으면 CORS 레이어가 헤더를 생략한다.
#[derive(Clone, Copy)]
pub struct ThrownError;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    AlreadyExists(String),
    #[error("{0}")]
    EmailNotVerified(String),
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    InvalidCredentials(String),
    #[error("{0}")]
    InvalidToken(String),
    #[error("{0}")]
    MethodNotAllowed(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    TokenExpired(String),
    #[error("{0}")]
    Unauthorized(String),
    /// 예상하지 못한 오류 (PHP Throwable) — 500 INTERNAL_ERROR
    #[error("{0}")]
    Internal(String),
}

impl AppError {
    /// ValidationException — 첫 번째 오류 메시지가 응답 message 가 된다
    pub fn validation(errors: Vec<String>) -> Self {
        Self::Validation(
            errors
                .into_iter()
                .next()
                .unwrap_or_else(|| "입력값 검증에 실패했습니다.".to_string()),
        )
    }
    pub fn email_not_verified() -> Self {
        Self::EmailNotVerified("이메일 인증이 필요합니다.".into())
    }
    pub fn forbidden() -> Self {
        Self::Forbidden("이 작업을 수행할 권한이 없습니다.".into())
    }
    pub fn invalid_credentials() -> Self {
        Self::InvalidCredentials("이메일 또는 비밀번호가 올바르지 않습니다.".into())
    }
    pub fn invalid_token() -> Self {
        Self::InvalidToken("유효하지 않은 토큰입니다.".into())
    }
    pub fn method_not_allowed() -> Self {
        Self::MethodNotAllowed("허용되지 않은 요청 메서드입니다.".into())
    }
    pub fn not_found() -> Self {
        Self::NotFound("요청한 리소스를 찾을 수 없습니다.".into())
    }
    pub fn token_expired() -> Self {
        Self::TokenExpired("토큰이 만료되었습니다.".into())
    }

    pub fn status_and_code(&self) -> (StatusCode, &'static str) {
        match self {
            Self::Validation(_) => (StatusCode::UNPROCESSABLE_ENTITY, "VALIDATION_ERROR"),
            Self::AlreadyExists(_) => (StatusCode::CONFLICT, "ALREADY_EXISTS"),
            Self::EmailNotVerified(_) => (StatusCode::FORBIDDEN, "EMAIL_NOT_VERIFIED"),
            Self::Forbidden(_) => (StatusCode::FORBIDDEN, "FORBIDDEN"),
            Self::InvalidCredentials(_) => (StatusCode::UNAUTHORIZED, "INVALID_CREDENTIALS"),
            Self::InvalidToken(_) => (StatusCode::UNAUTHORIZED, "INVALID_TOKEN"),
            Self::MethodNotAllowed(_) => (StatusCode::METHOD_NOT_ALLOWED, "METHOD_NOT_ALLOWED"),
            Self::NotFound(_) => (StatusCode::NOT_FOUND, "NOT_FOUND"),
            Self::TokenExpired(_) => (StatusCode::UNAUTHORIZED, "TOKEN_EXPIRED"),
            Self::Unauthorized(_) => (StatusCode::UNAUTHORIZED, "UNAUTHORIZED"),
            Self::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR"),
        }
    }
}

/// APP_DEBUG 여부 — 500 응답에 원본 메시지를 노출할지 결정 (main 에서 1회 설정)
pub static APP_DEBUG: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

pub const INTERNAL_MESSAGE: &str = "서버 내부 오류가 발생했습니다.";

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code) = self.status_and_code();
        // 메시지는 variant 가 가진 문자열을 그대로 빌린다 (Display 경유 재할당 없음)
        let message: &str = match &self {
            Self::Internal(msg) => {
                tracing::error!(error = %msg, "unhandled error");
                if *APP_DEBUG.get().unwrap_or(&false) {
                    msg
                } else {
                    INTERNAL_MESSAGE
                }
            }
            Self::Validation(m)
            | Self::AlreadyExists(m)
            | Self::EmailNotVerified(m)
            | Self::Forbidden(m)
            | Self::InvalidCredentials(m)
            | Self::InvalidToken(m)
            | Self::MethodNotAllowed(m)
            | Self::NotFound(m)
            | Self::TokenExpired(m)
            | Self::Unauthorized(m) => m,
        };
        let mut res = error_response(code, message, status);
        res.extensions_mut().insert(ThrownError);
        res
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        Self::Internal(e.to_string())
    }
}

impl From<redis::RedisError> for AppError {
    fn from(e: redis::RedisError) -> Self {
        Self::Internal(e.to_string())
    }
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        Self::Internal(e.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
