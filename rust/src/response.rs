//! 표준 JSON 응답 (PHP `JsonResponder`).
//!
//! PHP 는 `JSON_UNESCAPED_UNICODE | JSON_UNESCAPED_SLASHES` 로 인코딩하므로
//! serde_json 기본 직렬화(유니코드·슬래시 비이스케이프)와 바이트 단위로 같다.
//!
//! 응답 본문은 `serde_json::Value` 트리를 거치지 않고 `#[derive(Serialize)]` 타입에서
//! 바로 바이트로 직렬화한다 — 요청마다 키 문자열·맵 노드를 할당하지 않는다.
//! 필드 선언 순서가 곧 JSON 키 순서이므로 PHP 배열 키 순서와 맞춰 선언한다.

use axum::body::{Body, Bytes};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::Response;
use chrono::NaiveDateTime;
use serde::{Serialize, Serializer};

pub const JSON_CONTENT_TYPE: &str = "application/json; charset=utf-8";

/// 응답 본문 초기 버퍼 — 대부분의 응답이 재할당 없이 들어가는 크기
const BODY_CAPACITY: usize = 256;

/// 미리 만들어 둔 바이트를 그대로 JSON 응답으로 보낸다 (정적 응답용 — 직렬화·할당 없음)
pub fn json_bytes(body: Bytes, status: StatusCode) -> Response {
    let mut res = Response::new(Body::from(body));
    *res.status_mut() = status;
    res.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(JSON_CONTENT_TYPE),
    );
    res
}

pub fn json_response<T: Serialize + ?Sized>(payload: &T, status: StatusCode) -> Response {
    let mut body = Vec::with_capacity(BODY_CAPACITY);
    if serde_json::to_writer(&mut body, payload).is_err() {
        body.clear();
        body.extend_from_slice(b"null");
    }
    json_bytes(Bytes::from(body), status)
}

/// `{"status":"success","data":...}` (+ meta 가 있으면 `meta`)
#[derive(Serialize)]
struct Envelope<'a, T: Serialize, M: Serialize> {
    status: &'static str,
    data: &'a T,
    #[serde(skip_serializing_if = "Option::is_none")]
    meta: Option<&'a M>,
}

pub fn success<T: Serialize>(data: &T, status: StatusCode) -> Response {
    json_response(
        &Envelope::<T, ()> {
            status: "success",
            data,
            meta: None,
        },
        status,
    )
}

pub fn success_with_meta<T: Serialize, M: Serialize>(
    data: &T,
    meta: &M,
    status: StatusCode,
) -> Response {
    json_response(
        &Envelope {
            status: "success",
            data,
            meta: Some(meta),
        },
        status,
    )
}

pub fn ok<T: Serialize>(data: &T) -> Response {
    success(data, StatusCode::OK)
}

/// `{"status":"error","code":...,"message":...}`
#[derive(Serialize)]
struct ErrorBody<'a> {
    status: &'static str,
    code: &'a str,
    message: &'a str,
}

pub fn error_response(code: &str, message: &str, status: StatusCode) -> Response {
    json_response(
        &ErrorBody {
            status: "error",
            code,
            message,
        },
        status,
    )
}

/// 204 — 본문 없음. PHP SAPI 는 Content-Type 을 지정하지 않은 응답에 기본값
/// `text/html; charset=UTF-8` 을 붙이므로 동일하게 맞춘다.
pub fn no_content() -> Response {
    let mut res = Response::new(Body::empty());
    *res.status_mut() = StatusCode::NO_CONTENT;
    res.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=UTF-8"),
    );
    res
}

/// `format('Y-m-d H:i:s')` 직렬화 — 중간 String 없이 직렬화기에 바로 쓴다
pub fn ser_datetime<S: Serializer>(dt: &NaiveDateTime, s: S) -> Result<S::Ok, S::Error> {
    s.collect_str(&dt.format(crate::service::DATE_FORMAT))
}

pub fn ser_opt_datetime<S: Serializer>(
    dt: &Option<NaiveDateTime>,
    s: S,
) -> Result<S::Ok, S::Error> {
    match dt {
        Some(dt) => ser_datetime(dt, s),
        None => s.serialize_none(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn body_of(res: Response) -> String {
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn envelope_matches_php_layout() {
        #[derive(Serialize)]
        struct D {
            a: &'static str,
        }
        #[derive(Serialize)]
        struct M {
            page: i64,
        }
        assert_eq!(
            body_of(ok(&D { a: "한/b" })).await,
            r#"{"status":"success","data":{"a":"한/b"}}"#
        );
        assert_eq!(
            body_of(success_with_meta(
                &D { a: "x" },
                &M { page: 1 },
                StatusCode::OK
            ))
            .await,
            r#"{"status":"success","data":{"a":"x"},"meta":{"page":1}}"#
        );
        assert_eq!(
            body_of(error_response("NOT_FOUND", "없음", StatusCode::NOT_FOUND)).await,
            r#"{"status":"error","code":"NOT_FOUND","message":"없음"}"#
        );
    }
}
