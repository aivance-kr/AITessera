//! 비즈니스 로직 (PHP `AuthService` / `UserService` / `UserAdminService` / `LogService`).
//! 트랜잭션 경계·예외 종류·메시지는 PHP 구현과 같다.

use chrono::{Duration, NaiveDateTime};
use serde_json::{json, Map, Value};

use crate::domain::{
    validate_profile, Affiliation, CreateOperatorRequest, LogRequest, LoginRequest, PatchCommon,
    RegisterRequest, UpdateMeRequest, UpdateUserRequest, UserListQuery, UserRole,
};
use crate::error::{AppError, AppResult};
use crate::infra::{password_hash, password_verify};
use crate::jwt::{random_token, sha256_hex};
use crate::php;
use crate::repo::{self, FieldValue, ListFilter, ListRow, ManageRow, NewUser, ProfileRow, UserRow};
use crate::state::AppState;

pub const LOGIN_EVENT_QUEUE: &str = "login_event_queue";
pub const MAIL_QUEUE: &str = "mail_queue";
pub const LOG_QUEUE: &str = "log_queue";

const DATE_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

pub struct TokenPair {
    pub access_token: String,
    pub expires_in: i64,
    pub refresh_token: String,
}

impl TokenPair {
    /// 표준 토큰 응답 본문 (PHP `TokenPair::toArray`)
    pub fn to_json(&self) -> Value {
        json!({
            "access_token": self.access_token,
            "token_type": "Bearer",
            "expires_in": self.expires_in,
            "refresh_token": self.refresh_token,
        })
    }
}

pub struct LoginContext {
    pub ip: String,
    pub user_agent: String,
}

/// 인증 판정에 쓰는 사용자 (PHP `User`)
struct User {
    id: i64,
    email: String,
    password_hash: String,
    affiliation: Affiliation,
    role: UserRole,
    is_active: bool,
    email_verified: bool,
}

impl User {
    fn from_row(row: UserRow) -> AppResult<Self> {
        Ok(Self {
            id: row.id as i64,
            affiliation: Affiliation::from_db(&row.affiliation)?,
            role: UserRole::from_db(i64::from(row.role))?,
            email: row.email,
            password_hash: row.password_hash,
            is_active: row.is_active,
            email_verified: row.email_verified_at.is_some(),
        })
    }
}

/// `Rows::nullableString`
fn nullable_string(v: Option<String>) -> Value {
    match v {
        Some(s) if !s.is_empty() => Value::String(s),
        _ => Value::Null,
    }
}

/// `Rows::decodeJsonObject` + `$profile === [] ? null : $profile`
fn profile_value(v: Option<Value>) -> Value {
    match v {
        Some(v @ (Value::Object(_) | Value::Array(_))) if !php::is_empty_array(&v) => php::php_normalize(v),
        _ => Value::Null,
    }
}

fn fmt(dt: NaiveDateTime) -> String {
    dt.format(DATE_FORMAT).to_string()
}

/// 저장용 프로필 JSON — 빈 프로필은 NULL (PHP json_encode JSON_UNESCAPED_UNICODE)
fn profile_json(profile: &Value) -> Option<String> {
    if php::is_empty_array(profile) {
        None
    } else {
        Some(php::json_encode_unescaped_unicode(profile))
    }
}

// ───────────────────────────── AuthService ─────────────────────────────

pub async fn login(state: &AppState, req: LoginRequest, ctx: LoginContext) -> AppResult<TokenPair> {
    let email = req.email.clone();
    let result = login_inner(state, req).await;
    match &result {
        Ok(_) => record_login_event(state, &email, &ctx, true).await?,
        Err(AppError::InvalidCredentials(_) | AppError::EmailNotVerified(_)) => {
            record_login_event(state, &email, &ctx, false).await?
        }
        Err(_) => {}
    }
    result
}

async fn login_inner(state: &AppState, req: LoginRequest) -> AppResult<TokenPair> {
    let row = repo::find_active_by_email(&state.db, &req.email, req.affiliation.as_str())
        .await?
        .ok_or_else(AppError::invalid_credentials)?;
    let user = User::from_row(row)?;
    if !password_verify(req.password, user.password_hash.clone()).await? {
        return Err(AppError::invalid_credentials());
    }
    if !user.email_verified {
        return Err(AppError::email_not_verified());
    }
    let pair = issue_pair(state, &state.db, user.id, user.affiliation, user.role).await?;
    repo::update_last_login(&state.db, user.id, state.now()).await?;
    Ok(pair)
}

async fn record_login_event(state: &AppState, email: &str, ctx: &LoginContext, success: bool) -> AppResult<()> {
    state
        .queue
        .push(
            LOGIN_EVENT_QUEUE,
            &json!({
                "email": email,
                "ip": ctx.ip,
                "user_agent": ctx.user_agent,
                "success": success,
                "occurred_at": fmt(state.now()),
            }),
        )
        .await
}

pub async fn refresh(state: &AppState, refresh_token: &str) -> AppResult<TokenPair> {
    let token = repo::find_refresh_token(&state.db, &sha256_hex(refresh_token))
        .await?
        .ok_or_else(AppError::invalid_token)?;
    let now = state.now();
    // 폐기된 토큰 재사용 → 탈취 간주, 사용자 전체 토큰 무효화
    if token.closed_at.is_some() {
        repo::revoke_all_refresh_tokens(&state.db, token.user_id as i64, now).await?;
        return Err(AppError::InvalidToken("재사용이 감지되어 모든 세션이 무효화되었습니다.".into()));
    }
    if token.expires_at <= now {
        return Err(AppError::token_expired());
    }
    let user = repo::find_by_id(&state.db, token.user_id as i64)
        .await?
        .ok_or_else(AppError::invalid_token)?;
    let user = User::from_row(user)?;

    // 비활성(정지) 계정은 세션을 연장할 수 없다 — 남은 리프레시 토큰도 모두 폐기
    if !user.is_active {
        repo::revoke_all_refresh_tokens(&state.db, user.id, now).await?;
        return Err(AppError::invalid_token());
    }

    // 회전: 이전 토큰 폐기 + 새 토큰 발급을 하나의 트랜잭션으로
    let mut tx = state.db.begin().await?;
    repo::revoke_refresh_token(&mut *tx, token.id as i64, now).await?;
    let pair = issue_pair(state, &mut *tx, user.id, user.affiliation, user.role).await?;
    tx.commit().await?;
    Ok(pair)
}

pub async fn logout(state: &AppState, refresh_token: &str) -> AppResult<()> {
    let Some(token) = repo::find_refresh_token(&state.db, &sha256_hex(refresh_token)).await? else {
        return Ok(()); // 멱등 — 이미 없는 토큰
    };
    if token.closed_at.is_none() {
        repo::revoke_refresh_token(&state.db, token.id as i64, state.now()).await?;
    }
    Ok(())
}

async fn issue_pair(
    state: &AppState,
    db: impl sqlx::MySqlExecutor<'_>,
    user_id: i64,
    affiliation: Affiliation,
    role: UserRole,
) -> AppResult<TokenPair> {
    let access = state.jwt.issue_access_token(user_id, affiliation.as_str(), role)?;
    let refresh = random_token();
    let expires_at = state.now() + Duration::seconds(state.config.jwt_refresh_ttl);
    repo::store_refresh_token(db, user_id, &sha256_hex(&refresh), expires_at).await?;
    Ok(TokenPair { access_token: access, expires_in: state.jwt.access_ttl(), refresh_token: refresh })
}

// ───────────────────────────── UserService ─────────────────────────────

fn profile_to_json(row: ProfileRow) -> AppResult<Value> {
    Ok(json!({
        "id": row.id,
        "email": row.email,
        "name": row.name,
        "affiliation": Affiliation::from_db(&row.affiliation)?.as_str(),
        "role": UserRole::from_db(i64::from(row.role))?.value(),
        "contact": row.contact,
        "company": nullable_string(row.company),
        "profile": profile_value(row.profile),
        "email_verified": row.email_verified_at.is_some(),
        "created_at": fmt(row.created_at),
    }))
}

pub async fn me(state: &AppState, user_id: i64) -> AppResult<Value> {
    let row = repo::find_profile_by_id(&state.db, user_id)
        .await?
        .ok_or_else(|| AppError::NotFound("사용자를 찾을 수 없습니다.".into()))?;
    profile_to_json(row)
}

/// 공통 PATCH 필드 → UPDATE 컬럼 (name·contact·company·profile)
fn common_fields(common: &PatchCommon, affiliation: Affiliation) -> AppResult<Vec<(&'static str, FieldValue)>> {
    let mut fields = Vec::new();
    if let Some(name) = &common.name {
        fields.push(("name", FieldValue::Str(Some(name.clone()))));
    }
    if let Some(contact) = &common.contact {
        fields.push(("contact", FieldValue::Str(Some(contact.clone()))));
    }
    if common.has("company") {
        fields.push(("company", FieldValue::Str(common.company.clone())));
    }
    if common.has("profile") {
        let errors = validate_profile(affiliation, &common.profile);
        if !errors.is_empty() {
            return Err(AppError::validation(errors));
        }
        fields.push(("profile", FieldValue::Str(profile_json(&common.profile))));
    }
    Ok(fields)
}

async fn apply_update(
    state: &AppState,
    user_id: i64,
    fields: Vec<(&'static str, FieldValue)>,
    password: Option<String>,
    deactivating: bool,
) -> AppResult<()> {
    let hash = match password {
        Some(pw) => Some(password_hash(pw).await?),
        None => None,
    };
    let now = state.now();
    let mut tx = state.db.begin().await?;
    if !fields.is_empty() {
        repo::update_fields(&mut *tx, user_id, fields, now).await?;
    }
    if let Some(hash) = hash {
        repo::update_password(&mut *tx, user_id, &hash, now).await?;
        // 재설정 시 기존 세션을 삭제한다 — 폐기로 남기면 옛 기기의 refresh 가 재사용 감지를
        // 일으켜 회원이 새 비밀번호로 로그인한 세션까지 무효화된다
        repo::delete_all_refresh_tokens(&mut *tx, user_id).await?;
    } else if deactivating {
        // 비활성화 시 기존 세션(리프레시 토큰)을 즉시 끊는다
        repo::revoke_all_refresh_tokens(&mut *tx, user_id, now).await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn update_me(state: &AppState, req: UpdateMeRequest, user_id: i64) -> AppResult<Value> {
    let row = repo::find_by_id(&state.db, user_id)
        .await?
        .ok_or_else(|| AppError::NotFound("사용자를 찾을 수 없습니다.".into()))?;
    let user = User::from_row(row)?;
    // 비밀번호 변경 시 현재 비밀번호 검증
    if req.password.is_some() {
        // 비밀번호 변경은 새 세션을 발급하므로, 정지된 계정이 남은 Access 토큰으로 세션을 되살리지 못하게 막는다
        if !user.is_active {
            return Err(AppError::Forbidden("비활성화된 계정은 이 작업을 수행할 수 없습니다.".into()));
        }
        let ok = match &req.current_password {
            Some(current) => password_verify(current.clone(), user.password_hash.clone()).await?,
            None => false,
        };
        if !ok {
            return Err(AppError::InvalidCredentials("현재 비밀번호가 일치하지 않습니다.".into()));
        }
    }
    let fields = common_fields(&req.common, user.affiliation)?;
    let hash = match req.password {
        Some(pw) => Some(password_hash(pw).await?),
        None => None,
    };
    let now = state.now();
    let mut tx = state.db.begin().await?;
    if !fields.is_empty() {
        repo::update_fields(&mut *tx, user_id, fields, now).await?;
    }
    let tokens = match hash {
        Some(hash) => {
            repo::update_password(&mut *tx, user_id, &hash, now).await?;
            // 다른 기기 세션을 모두 끊고(삭제 — 재사용 감지 연쇄 무효화 방지), 현재 기기용 새 토큰 쌍을 발급한다
            repo::delete_all_refresh_tokens(&mut *tx, user_id).await?;
            Some(issue_pair(state, &mut *tx, user.id, user.affiliation, user.role).await?)
        }
        None => None,
    };
    tx.commit().await?;

    let mut profile = me(state, user_id).await?;
    if let (Some(pair), Some(obj)) = (tokens, profile.as_object_mut()) {
        obj.insert("tokens".into(), pair.to_json());
    }
    Ok(profile)
}

pub async fn withdraw(state: &AppState, user_id: i64, password: String) -> AppResult<()> {
    let row = repo::find_by_id(&state.db, user_id)
        .await?
        .ok_or_else(|| AppError::NotFound("사용자를 찾을 수 없습니다.".into()))?;
    let user = User::from_row(row)?;
    if !password_verify(password, user.password_hash).await? {
        return Err(AppError::InvalidCredentials("비밀번호가 일치하지 않습니다.".into()));
    }
    let now = state.now();
    let mut tx = state.db.begin().await?;
    repo::soft_delete(&mut *tx, user_id, now).await?;
    repo::revoke_all_refresh_tokens(&mut *tx, user_id, now).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn register(state: &AppState, req: &RegisterRequest) -> AppResult<i64> {
    if repo::email_exists(&state.db, &req.email, req.affiliation.as_str()).await? {
        return Err(AppError::AlreadyExists("이미 가입된 이메일입니다.".into()));
    }
    let now = state.now();
    let token = random_token();
    let expires_at = now + Duration::seconds(state.config.email_verify_ttl);
    let hash = password_hash(req.password.clone()).await?;

    let mut tx = state.db.begin().await?;
    let new_id = repo::create_user(
        &mut *tx,
        NewUser {
            email: &req.email,
            password_hash: &hash,
            affiliation: req.affiliation.as_str(),
            name: &req.name,
            contact: &req.contact,
            company: req.company.as_deref(),
            profile_json: profile_json(&req.profile),
            agreed_at: now,
            role: UserRole::Member, // 자가가입은 항상 일반회원
        },
    )
    .await?;
    repo::store_verification(&mut *tx, new_id, &sha256_hex(&token), expires_at).await?;
    tx.commit().await?;

    enqueue_verification_email(state, &req.email, &token).await?;
    Ok(new_id)
}

pub async fn create_operator_account(state: &AppState, req: &CreateOperatorRequest, creator_id: i64) -> AppResult<i64> {
    let creator = repo::find_by_id(&state.db, creator_id)
        .await?
        .ok_or_else(|| AppError::NotFound("생성 요청자를 찾을 수 없습니다.".into()))?;
    let creator = User::from_row(creator)?;
    // 정지된 계정은 토큰이 아직 유효해도 신규 계정을 만들 수 없다
    if !creator.is_active {
        return Err(AppError::Forbidden("비활성화된 계정은 이 작업을 수행할 수 없습니다.".into()));
    }
    // 소속 스코핑 — 자기 소속 계정만 생성 가능
    if creator.affiliation != req.affiliation {
        return Err(AppError::Forbidden("본인 소속의 계정만 생성할 수 있습니다.".into()));
    }
    if repo::email_exists(&state.db, &req.email, req.affiliation.as_str()).await? {
        return Err(AppError::AlreadyExists("이미 가입된 이메일입니다.".into()));
    }
    let now = state.now();
    let hash = password_hash(req.password.clone()).await?;
    let mut tx = state.db.begin().await?;
    let new_id = repo::create_user(
        &mut *tx,
        NewUser {
            email: &req.email,
            password_hash: &hash,
            affiliation: req.affiliation.as_str(),
            name: &req.name,
            contact: &req.contact,
            company: req.company.as_deref(),
            profile_json: profile_json(&req.profile),
            agreed_at: now,
            role: req.role,
        },
    )
    .await?;
    // 운영자가 신뢰 채널로 생성 → 이메일 인증 즉시 완료
    repo::mark_email_verified(&mut *tx, new_id, now).await?;
    tx.commit().await?;
    Ok(new_id)
}

pub async fn verify_email(state: &AppState, token: &str) -> AppResult<()> {
    let v = repo::find_verification(&state.db, &sha256_hex(token))
        .await?
        .ok_or_else(AppError::invalid_token)?;
    if v.closed_at.is_some() {
        return Err(AppError::InvalidToken("이미 사용된 인증 토큰입니다.".into()));
    }
    let now = state.now();
    if v.expires_at <= now {
        return Err(AppError::TokenExpired("인증 토큰이 만료되었습니다.".into()));
    }
    let mut tx = state.db.begin().await?;
    repo::mark_email_verified(&mut *tx, v.user_id as i64, now).await?;
    repo::consume_verification(&mut *tx, v.id as i64, now).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn resend_verification(state: &AppState, email: &str, affiliation: &str) -> AppResult<()> {
    let Some(row) = repo::find_active_by_email(&state.db, email, affiliation).await? else {
        return Ok(());
    };
    let user = User::from_row(row)?;
    if user.email_verified {
        return Ok(());
    }
    let token = random_token();
    let expires_at = state.now() + Duration::seconds(state.config.email_verify_ttl);
    let mut tx = state.db.begin().await?;
    repo::delete_unconsumed_verifications(&mut *tx, user.id).await?;
    repo::store_verification(&mut *tx, user.id, &sha256_hex(&token), expires_at).await?;
    tx.commit().await?;
    enqueue_verification_email(state, &user.email, &token).await
}

async fn enqueue_verification_email(state: &AppState, email: &str, token: &str) -> AppResult<()> {
    state
        .queue
        .push(MAIL_QUEUE, &json!({ "type": "email_verification", "to": email, "token": token }))
        .await
}

// ───────────────────────────── UserAdminService ─────────────────────────────

fn list_item(row: ListRow) -> AppResult<Value> {
    Ok(json!({
        "id": row.id,
        "email": row.email,
        "name": row.name,
        "affiliation": Affiliation::from_db(&row.affiliation)?.as_str(),
        "role": UserRole::from_db(i64::from(row.role))?.value(),
        "is_active": row.is_active,
        "email_verified": row.email_verified_at.is_some(),
        "created_at": fmt(row.created_at),
    }))
}

/// AdminUserView::toArray
fn manage_view(row: ManageRow) -> AppResult<(Affiliation, Value)> {
    let affiliation = Affiliation::from_db(&row.affiliation)?;
    Ok((
        affiliation,
        json!({
            "id": row.id,
            "email": row.email,
            "name": row.name,
            "affiliation": affiliation.as_str(),
            "role": UserRole::from_db(i64::from(row.role))?.value(),
            "is_active": row.is_active,
            "contact": nullable_string(row.contact),
            "company": nullable_string(row.company),
            "profile": profile_value(row.profile),
            "email_verified": row.email_verified_at.is_some(),
            "last_login_at": row.last_login_at.map(fmt),
            "created_at": fmt(row.created_at),
        }),
    ))
}

async fn require_active_operator(state: &AppState, operator_id: i64) -> AppResult<User> {
    let row = repo::find_by_id(&state.db, operator_id)
        .await?
        .ok_or_else(|| AppError::NotFound("요청자를 찾을 수 없습니다.".into()))?;
    let operator = User::from_row(row)?;
    if !operator.is_active {
        return Err(AppError::Forbidden("비활성화된 계정은 이 작업을 수행할 수 없습니다.".into()));
    }
    Ok(operator)
}

pub async fn list_users(state: &AppState, query: &UserListQuery, operator_id: i64) -> AppResult<(Vec<Value>, i64)> {
    let operator = require_active_operator(state, operator_id).await?;
    let filter = ListFilter {
        affiliation: operator.affiliation.as_str(),
        role: query.role.map(UserRole::value),
        is_active: query.is_active,
        search: query.search.as_deref(),
    };
    let rows = repo::paginate_by_affiliation(
        &state.db,
        &filter,
        query.sort_column,
        query.sort_direction,
        query.per_page,
        query.offset(),
    )
    .await?;
    let total = repo::count_by_affiliation(&state.db, &filter).await?;
    let items = rows.into_iter().map(list_item).collect::<AppResult<Vec<_>>>()?;
    Ok((items, total))
}

pub async fn get_user(state: &AppState, target_id: i64, operator_id: i64) -> AppResult<Value> {
    let operator = require_active_operator(state, operator_id).await?;
    let row = repo::find_manageable_by_id(&state.db, target_id, operator.affiliation.as_str())
        .await?
        .ok_or_else(|| AppError::NotFound("회원을 찾을 수 없습니다.".into()))?;
    Ok(manage_view(row)?.1)
}

pub async fn update_user(state: &AppState, req: UpdateUserRequest, target_id: i64, operator_id: i64) -> AppResult<()> {
    let operator = require_active_operator(state, operator_id).await?;
    let row = repo::find_manageable_by_id(&state.db, target_id, operator.affiliation.as_str())
        .await?
        .ok_or_else(|| AppError::NotFound("회원을 찾을 수 없습니다.".into()))?;
    let (target_affiliation, _) = manage_view(row)?;
    if target_id == operator_id {
        if req.common.has("role") {
            return Err(AppError::Forbidden("본인 계정의 회원구분은 변경할 수 없습니다.".into()));
        }
        if req.common.has("is_active") {
            return Err(AppError::Forbidden("본인 계정의 활성 상태는 변경할 수 없습니다.".into()));
        }
    }
    let mut fields = common_fields(&req.common, target_affiliation)?;
    if let Some(role) = req.role {
        fields.push(("role", FieldValue::Int(role.value())));
    }
    if let Some(active) = req.is_active {
        fields.push(("is_active", FieldValue::Int(i64::from(active))));
    }
    let deactivating = req.is_active == Some(false);
    apply_update(state, target_id, fields, req.password, deactivating).await
}

// ───────────────────────────── LogService ─────────────────────────────

pub async fn record_log(state: &AppState, req: &LogRequest) -> AppResult<()> {
    state.queue.push(LOG_QUEUE, &req.to_payload()).await
}

/// 목록 응답 meta (페이지네이션 4필드)
pub fn pagination_meta(query: &UserListQuery, total: i64) -> Map<String, Value> {
    let last_page = ((total as f64) / (query.per_page as f64)).ceil().max(1.0) as i64;
    let mut meta = Map::new();
    meta.insert("page".into(), json!(query.page));
    meta.insert("per_page".into(), json!(query.per_page));
    meta.insert("total".into(), json!(total));
    meta.insert("last_page".into(), json!(last_page));
    meta
}
