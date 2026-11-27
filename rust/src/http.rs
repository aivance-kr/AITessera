//! 라우팅·미들웨어·핸들러 (PHP `config/routes.php` + `src/Middleware` + `src/Controller`).
//!
//! 미들웨어 순서는 PHP 파이프라인과 같다:
//! `ErrorHandler(panic) → TrailingSlash → Cors → RateLimit → JwtAuth → (RoleGuard) → RouteDispatch`.
//! 공개/보호 구분도 라우트 매칭 전에 (method, path) 정확 일치로 판정하므로,
//! 토큰 없이 존재하지 않는 경로를 호출하면 404 가 아니라 401 이 되는 점까지 같다.

use std::net::SocketAddr;

use axum::body::{Body, Bytes};
use axum::extract::{ConnectInfo, FromRequest, Path, RawQuery, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Router};
use serde_json::{json, Map, Value};
use tower_http::catch_panic::CatchPanicLayer;

use crate::domain::{
    Affiliation, CreateOperatorRequest, LogRequest, LoginRequest, RegisterRequest, UpdateMeRequest,
    UpdateUserRequest, UserListQuery, UserRole,
};
use crate::error::{error_response, AppError, AppResult, ThrownError};
use crate::jwt::AuthContext;
use crate::php::{self, Input};
use crate::response::{no_content, ok, success};
use crate::service::{self, LoginContext, TokenPair};
use crate::state::SharedState;

const APP_NAME: &str = "AITessera";
const APP_VERSION: &str = "1.0.0";
const OPENAPI_JSON: &str = include_str!("../static/openapi.json");

/// 인증 없이 접근 가능한 (method, path) — PHP JwtAuthMiddleware::PUBLIC_ROUTES
const PUBLIC_ROUTES: [(&str, &str); 12] = [
    ("GET", "/health"),
    ("POST", "/api/v1/tokens"),
    ("POST", "/api/v1/tokens/refresh"),
    ("DELETE", "/api/v1/tokens"),
    ("POST", "/api/v1/users"),
    ("POST", "/api/v1/users/verify"),
    ("POST", "/api/v1/users/verify/resend"),
    ("POST", "/api/v1/logs"),
    ("GET", "/api/docs"),
    ("GET", "/api/v1/openapi.json"),
    ("GET", "/.well-known/jwks.json"),
    ("GET", "/api/v1/jwks.json"),
];

/// 엄격한 레이트 리밋 대상 — PHP RateLimitMiddleware::SENSITIVE_ROUTES
const SENSITIVE_ROUTES: [(&str, &str); 5] = [
    ("POST", "/api/v1/tokens"),
    ("POST", "/api/v1/tokens/refresh"),
    ("POST", "/api/v1/users"),
    ("POST", "/api/v1/users/verify"),
    ("POST", "/api/v1/users/verify/resend"),
];

pub fn router(state: SharedState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/docs", get(docs_ui))
        .route("/api/v1/openapi.json", get(docs_spec))
        .route("/.well-known/jwks.json", get(jwks))
        .route("/api/v1/jwks.json", get(jwks))
        .route("/api/v1/tokens", post(token_issue).delete(token_revoke))
        .route("/api/v1/tokens/refresh", post(token_refresh))
        .route("/api/v1/users", post(user_register).get(admin_index))
        .route("/api/v1/users/verify", post(user_verify))
        .route("/api/v1/users/verify/resend", post(user_resend))
        .route("/api/v1/users/{id}", get(admin_show).patch(admin_update))
        .route("/api/v1/logs", post(log_store))
        .route("/api/v1/me", get(me_show).patch(me_update).delete(me_destroy))
        .route("/api/v1/operators", post(operator_create))
        .fallback(|| async { AppError::not_found() })
        // 안쪽 → 바깥쪽 순으로 적용된다 (마지막 layer 가 가장 바깥)
        .layer(middleware::from_fn(method_not_allowed))
        .layer(middleware::from_fn_with_state(state.clone(), jwt_auth))
        .layer(middleware::from_fn_with_state(state.clone(), rate_limit))
        .layer(middleware::from_fn(cors))
        // 가장 바깥 — panic → 500 INTERNAL_ERROR (PHP ErrorHandler 의 Throwable 분기)
        .layer(CatchPanicLayer::custom(|_: Box<dyn std::any::Any + Send + 'static>| {
            let mut res = error_response(
                "INTERNAL_ERROR",
                crate::error::INTERNAL_MESSAGE,
                StatusCode::INTERNAL_SERVER_ERROR,
            );
            res.extensions_mut().insert(ThrownError);
            res
        }))
        .layer(axum::extract::DefaultBodyLimit::max(8 * 1024 * 1024))
        .with_state(state)
}

// ───────────────────────────── 미들웨어 ─────────────────────────────

/// CorsMiddleware — 프리플라이트는 즉시 204, 정상 응답에만 CORS 헤더 부착
async fn cors(req: Request, next: Next) -> Response {
    let mut res = if req.method() == Method::OPTIONS {
        no_content()
    } else {
        let res = next.run(req).await;
        // PHP 에서는 예외가 Cors 를 통과해 ErrorHandler 에서 응답이 만들어지므로 헤더가 없다
        if res.extensions().get::<ThrownError>().is_some() {
            return res;
        }
        res
    };
    let h = res.headers_mut();
    h.insert("access-control-allow-origin", HeaderValue::from_static("*"));
    h.insert(
        "access-control-allow-methods",
        HeaderValue::from_static("GET, POST, PUT, PATCH, DELETE, OPTIONS"),
    );
    h.insert("access-control-allow-headers", HeaderValue::from_static("Authorization, Content-Type"));
    h.insert("access-control-max-age", HeaderValue::from_static("86400"));
    res
}

/// RateLimitMiddleware — IP 별 sliding window (인증 계열 엄격 / 일반 API)
async fn rate_limit(State(state): State<SharedState>, req: Request, next: Next) -> Response {
    let path = req.uri().path().to_string();
    if path == "/health" {
        return next.run(req).await;
    }
    let method = req.method().as_str().to_string();
    let sensitive = SENSITIVE_ROUTES.iter().any(|(m, p)| *m == method && *p == path);
    let (id, limit) = if sensitive {
        ("auth", state.config.rate_limit_auth)
    } else {
        ("api", state.config.rate_limit_api)
    };
    match state.limiter.consume(id, &client_ip(&req), limit).await {
        Ok(None) => next.run(req).await,
        Ok(Some(retry_after)) => {
            let mut res = error_response(
                "RATE_LIMITED",
                "요청이 너무 많습니다. 잠시 후 다시 시도해 주세요.",
                StatusCode::TOO_MANY_REQUESTS,
            );
            if let Ok(v) = HeaderValue::from_str(&retry_after.to_string()) {
                res.headers_mut().insert(header::RETRY_AFTER, v);
            }
            res
        }
        Err(e) => e.into_response(),
    }
}

/// JwtAuthMiddleware — 공개 경로 외에는 Bearer 토큰 검증 후 인증 정보를 요청에 싣는다
async fn jwt_auth(State(state): State<SharedState>, mut req: Request, next: Next) -> Response {
    let method = req.method().as_str();
    let path = req.uri().path();
    if PUBLIC_ROUTES.iter().any(|(m, p)| *m == method && *p == path) {
        return next.run(req).await;
    }
    let header_line = header_line(req.headers(), header::AUTHORIZATION);
    match state.jwt.authenticate(&header_line) {
        Ok(auth) => {
            req.extensions_mut().insert(auth);
            next.run(req).await
        }
        Err(e) => e.into_response(),
    }
}

/// 라우트 매칭 실패의 표준화 — axum 기본 405(빈 본문)를 METHOD_NOT_ALLOWED 로 바꾸고,
/// FastRoute `{id:\d+}` 처럼 숫자가 아닌 회원 ID 경로는 404 로 처리한다.
async fn method_not_allowed(req: Request, next: Next) -> Response {
    let non_digit_user_path = req
        .uri()
        .path()
        .strip_prefix("/api/v1/users/")
        .is_some_and(|rest| !rest.contains('/') && !is_digits(rest) && rest != "verify");
    let res = next.run(req).await;
    if res.status() == StatusCode::METHOD_NOT_ALLOWED && res.extensions().get::<ThrownError>().is_none() {
        return if non_digit_user_path {
            AppError::not_found().into_response()
        } else {
            AppError::method_not_allowed().into_response()
        };
    }
    res
}

fn is_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// REMOTE_ADDR 만 신뢰한다 (X-Forwarded-For 미사용 — PHP 와 동일 정책)
fn client_ip(req: &Request) -> String {
    req.extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(addr)| addr.ip().to_string())
        .unwrap_or_else(|| "0.0.0.0".into())
}

/// PSR-7 `getHeaderLine()` — 같은 이름의 헤더를 ", " 로 결합
fn header_line(headers: &HeaderMap, name: header::HeaderName) -> String {
    headers
        .get_all(name)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .collect::<Vec<_>>()
        .join(", ")
}

/// RoleGuardMiddleware (#[RequireRole(Operator)]) — 토큰의 회원구분으로 판정
fn require_operator(auth: &AuthContext) -> AppResult<()> {
    if auth.role == UserRole::Operator {
        Ok(())
    } else {
        Err(AppError::forbidden())
    }
}

// ───────────────────────────── 요청 본문 ─────────────────────────────

/// BaseController::jsonInput — POST 폼(urlencoded)은 폼 필드, 그 외는 JSON 본문.
/// 디코드 실패·비배열 JSON 은 빈 배열로 취급한다.
pub struct JsonInput(pub Input);

impl<S: Send + Sync> FromRequest<S> for JsonInput {
    type Rejection = Response;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let is_post = req.method() == Method::POST;
        let content_type = header_line(req.headers(), header::CONTENT_TYPE);
        let base_type = content_type.split(';').next().unwrap_or("").trim().to_lowercase();
        let body = Bytes::from_request(req, state).await.map_err(IntoResponse::into_response)?;

        if is_post && base_type == "application/x-www-form-urlencoded" {
            return Ok(Self(form_to_input(&body)));
        }
        if is_post && base_type == "multipart/form-data" {
            // multipart 필드 파싱은 미지원 — 이 API 의 모든 클라이언트는 JSON 을 보낸다
            return Ok(Self(Map::new()));
        }
        if body.is_empty() {
            return Ok(Self(Map::new()));
        }
        let input = match serde_json::from_slice::<Value>(&body) {
            Ok(Value::Object(map)) => map,
            Ok(Value::Array(list)) => list.into_iter().enumerate().map(|(i, v)| (i.to_string(), v)).collect(),
            _ => Map::new(),
        };
        Ok(Self(input))
    }
}

/// PHP `$_POST` / `$_GET` 해석 — 같은 키는 마지막 값, `key[]` 형태는 배열
fn form_to_input(raw: &[u8]) -> Input {
    let pairs: Vec<(String, String)> = serde_urlencoded::from_bytes(raw).unwrap_or_default();
    let mut map = Map::new();
    for (key, value) in pairs {
        match key.find('[') {
            Some(idx) if idx > 0 => {
                let base = key[..idx].to_string();
                let entry = map.entry(base).or_insert_with(|| Value::Array(vec![]));
                match entry {
                    Value::Array(list) => list.push(Value::String(value)),
                    other => *other = Value::Array(vec![Value::String(value)]),
                }
            }
            _ => {
                map.insert(key, Value::String(value));
            }
        }
    }
    map
}

// ───────────────────────────── 핸들러: 공개 ─────────────────────────────

async fn health() -> Response {
    ok(json!({ "service": APP_NAME, "version": APP_VERSION, "status": "ok" }))
}

fn raw_response(body: impl Into<Body>, content_type: &'static str, cache_control: &'static str) -> Response {
    let mut res = Response::new(body.into());
    res.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    res.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static(cache_control));
    res
}

async fn docs_spec() -> Response {
    raw_response(OPENAPI_JSON, "application/json; charset=utf-8", "no-store")
}

async fn docs_ui() -> Response {
    raw_response(DOCS_HTML, "text/html; charset=utf-8", "no-store")
}

async fn jwks(State(state): State<SharedState>) -> Response {
    raw_response(
        state.jwt.jwks_body.clone(),
        "application/jwk-set+json; charset=utf-8",
        "public, max-age=3600",
    )
}

fn token_response(pair: TokenPair, status: StatusCode) -> Response {
    success(
        json!({
            "access_token": pair.access_token,
            "token_type": "Bearer",
            "expires_in": pair.expires_in,
            "refresh_token": pair.refresh_token,
        }),
        None,
        status,
    )
}

async fn token_issue(
    State(state): State<SharedState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    JsonInput(input): JsonInput,
) -> AppResult<Response> {
    let dto = LoginRequest::from_input(&input)?;
    let ctx = LoginContext {
        ip: addr.ip().to_string(),
        user_agent: header_line(&headers, header::USER_AGENT),
    };
    let pair = service::login(&state, dto, ctx).await?;
    Ok(token_response(pair, StatusCode::CREATED))
}

fn require_refresh_token(input: &Input) -> AppResult<String> {
    php::non_empty_str(input.get("refresh_token"))
        .map(str::to_string)
        .ok_or_else(|| AppError::validation(vec!["refresh_token 은 필수입니다.".into()]))
}

async fn token_refresh(State(state): State<SharedState>, JsonInput(input): JsonInput) -> AppResult<Response> {
    let token = require_refresh_token(&input)?;
    let pair = service::refresh(&state, &token).await?;
    Ok(token_response(pair, StatusCode::OK))
}

async fn token_revoke(State(state): State<SharedState>, JsonInput(input): JsonInput) -> AppResult<Response> {
    let token = require_refresh_token(&input)?;
    service::logout(&state, &token).await?;
    Ok(no_content())
}

async fn user_register(State(state): State<SharedState>, JsonInput(input): JsonInput) -> AppResult<Response> {
    let dto = RegisterRequest::from_input(&input)?;
    let id = service::register(&state, &dto).await?;
    Ok(success(
        json!({ "id": id, "email": dto.email, "email_verified": false }),
        None,
        StatusCode::CREATED,
    ))
}

fn require_string_field(input: &Input, field: &str) -> AppResult<String> {
    php::non_empty_str(input.get(field))
        .map(str::to_string)
        .ok_or_else(|| AppError::validation(vec![format!("{field} 은(는) 필수입니다.")]))
}

async fn user_verify(State(state): State<SharedState>, JsonInput(input): JsonInput) -> AppResult<Response> {
    let token = require_string_field(&input, "token")?;
    service::verify_email(&state, &token).await?;
    Ok(ok(json!({ "verified": true })))
}

async fn user_resend(State(state): State<SharedState>, JsonInput(input): JsonInput) -> AppResult<Response> {
    let email = require_string_field(&input, "email")?;
    let affiliation = input
        .get("affiliation")
        .and_then(Value::as_str)
        .and_then(Affiliation::try_from_str)
        .ok_or_else(|| AppError::validation(vec!["affiliation 은(는) 필수이며 유효한 소속이어야 합니다.".into()]))?;
    service::resend_verification(&state, &email, affiliation.as_str()).await?;
    // 이메일 존재 여부를 노출하지 않도록 항상 202
    Ok(success(json!({ "message": "인증 메일을 발송했습니다." }), None, StatusCode::ACCEPTED))
}

async fn log_store(State(state): State<SharedState>, JsonInput(input): JsonInput) -> AppResult<Response> {
    let dto = LogRequest::from_input(&input)?;
    service::record_log(&state, &dto).await?;
    Ok(success(json!({ "accepted": true }), None, StatusCode::ACCEPTED))
}

// ───────────────────────────── 핸들러: 보호 ─────────────────────────────

async fn me_show(State(state): State<SharedState>, Extension(auth): Extension<AuthContext>) -> AppResult<Response> {
    Ok(ok(service::me(&state, auth.user_id).await?))
}

async fn me_update(
    State(state): State<SharedState>,
    Extension(auth): Extension<AuthContext>,
    JsonInput(input): JsonInput,
) -> AppResult<Response> {
    let dto = UpdateMeRequest::from_input(&input)?;
    Ok(ok(service::update_me(&state, dto, auth.user_id).await?))
}

async fn me_destroy(
    State(state): State<SharedState>,
    Extension(auth): Extension<AuthContext>,
    JsonInput(input): JsonInput,
) -> AppResult<Response> {
    // 본인 확인용 비밀번호 — 누락은 401 이 아니라 422
    let password = php::non_empty_str(input.get("password"))
        .map(str::to_string)
        .ok_or_else(|| AppError::validation(vec!["password 는 필수입니다.".into()]))?;
    service::withdraw(&state, auth.user_id, password).await?;
    Ok(no_content())
}

async fn operator_create(
    State(state): State<SharedState>,
    Extension(auth): Extension<AuthContext>,
    JsonInput(input): JsonInput,
) -> AppResult<Response> {
    require_operator(&auth)?;
    let dto = CreateOperatorRequest::from_input(&input)?;
    let id = service::create_operator_account(&state, &dto, auth.user_id).await?;
    Ok(success(
        json!({ "id": id, "email": dto.email, "role": dto.role.value(), "email_verified": true }),
        None,
        StatusCode::CREATED,
    ))
}

async fn admin_index(
    State(state): State<SharedState>,
    Extension(auth): Extension<AuthContext>,
    RawQuery(query): RawQuery,
) -> AppResult<Response> {
    require_operator(&auth)?;
    let params = form_to_input(query.unwrap_or_default().as_bytes());
    let q = UserListQuery::from_params(&params)?;
    let (items, total) = service::list_users(&state, &q, auth.user_id).await?;
    Ok(success(Value::Array(items), Some(service::pagination_meta(&q, total)), StatusCode::OK))
}

/// `{id:\d+}` — 숫자가 아니면 라우트 불일치(404). (int) 캐스트처럼 범위 초과는 포화한다.
fn path_id(raw: &str) -> AppResult<i64> {
    if !is_digits(raw) {
        return Err(AppError::not_found());
    }
    Ok(raw.parse::<i64>().unwrap_or(i64::MAX))
}

async fn admin_show(
    State(state): State<SharedState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> AppResult<Response> {
    let target = path_id(&id)?;
    require_operator(&auth)?;
    Ok(ok(service::get_user(&state, target, auth.user_id).await?))
}

async fn admin_update(
    State(state): State<SharedState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
    JsonInput(input): JsonInput,
) -> AppResult<Response> {
    let target = path_id(&id)?;
    require_operator(&auth)?;
    let dto = UpdateUserRequest::from_input(&input)?;
    service::update_user(&state, dto, target, auth.user_id).await?;
    // 수정 후 최신 상태 반환
    Ok(ok(service::get_user(&state, target, auth.user_id).await?))
}

/// DocsController::html() — PHP heredoc 결과와 바이트 단위로 같다
const DOCS_HTML: &str = r#"<!DOCTYPE html>
<html lang="ko">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>AITessera API 문서</title>
  <script type="module" src="https://unpkg.com/rapidoc/dist/rapidoc-min.js"></script>
  <style>body { margin: 0; }</style>
</head>
<body>
  <rapi-doc
    spec-url="/api/v1/openapi.json"
    render-style="focused"
    nav-item-spacing="relaxed"
    show-header="false"
    allow-spec-url-load="false"
    allow-spec-file-load="false"
    allow-authentication="true"
    persist-auth="true"
    allow-try="true"
    schema-style="table"
    schema-expand-level="10"
    default-schema-tab="schema"
    theme="light"
    regular-font="-apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif"
    style="height:100vh; width:100%;"
  ></rapi-doc>
</body>
</html>"#;
