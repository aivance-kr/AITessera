//! 표준 JSON 응답 (PHP `JsonResponder`).
//!
//! PHP 는 `JSON_UNESCAPED_UNICODE | JSON_UNESCAPED_SLASHES` 로 인코딩하므로
//! serde_json 기본 직렬화(유니코드·슬래시 비이스케이프)와 바이트 단위로 같다.

use axum::body::Body;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::Response;
use serde_json::{Map, Value};

pub const JSON_CONTENT_TYPE: &str = "application/json; charset=utf-8";

pub fn json_response(payload: &Value, status: StatusCode) -> Response {
    let body = serde_json::to_vec(payload).unwrap_or_else(|_| b"null".to_vec());
    let mut res = Response::new(Body::from(body));
    *res.status_mut() = status;
    res.headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(JSON_CONTENT_TYPE));
    res
}

/// `{"status":"success","data":...}` (+ meta 가 비어 있지 않으면 `meta`)
pub fn success(data: Value, meta: Option<Map<String, Value>>, status: StatusCode) -> Response {
    let mut payload = Map::new();
    payload.insert("status".into(), Value::String("success".into()));
    payload.insert("data".into(), data);
    if let Some(meta) = meta.filter(|m| !m.is_empty()) {
        payload.insert("meta".into(), Value::Object(meta));
    }
    json_response(&Value::Object(payload), status)
}

pub fn ok(data: Value) -> Response {
    success(data, None, StatusCode::OK)
}

/// 204 — 본문 없음. PHP SAPI 는 Content-Type 을 지정하지 않은 응답에 기본값
/// `text/html; charset=UTF-8` 을 붙이므로 동일하게 맞춘다.
pub fn no_content() -> Response {
    let mut res = Response::new(Body::empty());
    *res.status_mut() = StatusCode::NO_CONTENT;
    res.headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static("text/html; charset=UTF-8"));
    res
}
