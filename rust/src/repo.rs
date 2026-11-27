//! 데이터 접근 계층 (PHP `App\Repository\*`) — 모든 SQL 은 바인딩만 사용한다.
//!
//! 컬럼 목록·WHERE 조건은 PHP Repository 와 같다. 정렬 컬럼·방향과 UPDATE 대상 컬럼은
//! 화이트리스트 상수에서만 오므로 문자열 조립에 사용자 입력이 들어가지 않는다.

use chrono::NaiveDateTime;
use serde_json::Value;
use sqlx::{FromRow, MySqlExecutor};

use crate::domain::UserRole;
use crate::error::AppResult;

// ───────────────────────────── 행 타입 ─────────────────────────────

/// `COLUMNS` — 인증·권한 판정용
#[derive(FromRow)]
pub struct UserRow {
    pub id: u64,
    pub email: String,
    pub password_hash: String,
    pub affiliation: String,
    pub role: u8,
    pub is_active: bool,
    pub email_verified_at: Option<NaiveDateTime>,
}

/// `PROFILE_COLUMNS` — /me 응답용
#[derive(FromRow)]
pub struct ProfileRow {
    pub id: u64,
    pub email: String,
    pub name: String,
    pub affiliation: String,
    pub role: u8,
    pub contact: String,
    pub company: Option<String>,
    pub profile: Option<Value>,
    pub email_verified_at: Option<NaiveDateTime>,
    pub created_at: NaiveDateTime,
}

/// `LIST_COLUMNS` — 회원 목록
#[derive(FromRow)]
pub struct ListRow {
    pub id: u64,
    pub email: String,
    pub name: String,
    pub affiliation: String,
    pub role: u8,
    pub is_active: bool,
    pub email_verified_at: Option<NaiveDateTime>,
    pub created_at: NaiveDateTime,
}

/// `MANAGE_COLUMNS` — 회원 상세(운영자)
#[derive(FromRow)]
pub struct ManageRow {
    pub id: u64,
    pub email: String,
    pub name: String,
    pub affiliation: String,
    pub role: u8,
    pub is_active: bool,
    pub contact: Option<String>,
    pub company: Option<String>,
    pub profile: Option<Value>,
    pub email_verified_at: Option<NaiveDateTime>,
    pub last_login_at: Option<NaiveDateTime>,
    pub created_at: NaiveDateTime,
}

#[derive(FromRow)]
pub struct TokenRow {
    pub id: u64,
    pub user_id: u64,
    pub expires_at: NaiveDateTime,
    /// refresh_tokens.revoked_at / email_verifications.consumed_at
    pub closed_at: Option<NaiveDateTime>,
}

// ───────────────────────────── users ─────────────────────────────

pub async fn find_active_by_email(
    db: impl MySqlExecutor<'_>,
    email: &str,
    affiliation: &str,
) -> AppResult<Option<UserRow>> {
    Ok(sqlx::query_as::<_, UserRow>(
        "SELECT id, email, password_hash, affiliation, role, is_active, email_verified_at FROM users
         WHERE email = ? AND affiliation = ? AND is_active = 1 AND deleted_at IS NULL
         LIMIT 1",
    )
    .bind(email)
    .bind(affiliation)
    .fetch_optional(db)
    .await?)
}

pub async fn find_by_id(db: impl MySqlExecutor<'_>, id: i64) -> AppResult<Option<UserRow>> {
    Ok(sqlx::query_as::<_, UserRow>(
        "SELECT id, email, password_hash, affiliation, role, is_active, email_verified_at FROM users
         WHERE id = ? AND deleted_at IS NULL
         LIMIT 1",
    )
    .bind(id)
    .fetch_optional(db)
    .await?)
}

/// find_by_id + 행 잠금(FOR UPDATE) — 트랜잭션 안에서만 사용 (PHP `lockById`)
pub async fn lock_by_id(db: impl MySqlExecutor<'_>, id: i64) -> AppResult<Option<UserRow>> {
    Ok(sqlx::query_as::<_, UserRow>(
        "SELECT id, email, password_hash, affiliation, role, is_active, email_verified_at FROM users
         WHERE id = ? AND deleted_at IS NULL
         LIMIT 1
         FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(db)
    .await?)
}

pub async fn find_profile_by_id(
    db: impl MySqlExecutor<'_>,
    id: i64,
) -> AppResult<Option<ProfileRow>> {
    Ok(sqlx::query_as::<_, ProfileRow>(
        "SELECT id, email, name, affiliation, role, contact, company, profile, email_verified_at, created_at FROM users
         WHERE id = ? AND deleted_at IS NULL
         LIMIT 1",
    )
    .bind(id)
    .fetch_optional(db)
    .await?)
}

/// 탈퇴 회원 이메일은 점유로 보지 않고, 소속이 다르면 별개 계정으로 본다
pub async fn email_exists(
    db: impl MySqlExecutor<'_>,
    email: &str,
    affiliation: &str,
) -> AppResult<bool> {
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT 1 FROM users WHERE email = ? AND affiliation = ? AND deleted_at IS NULL LIMIT 1",
    )
    .bind(email)
    .bind(affiliation)
    .fetch_optional(db)
    .await?;
    Ok(row.is_some())
}

pub struct NewUser<'a> {
    pub email: &'a str,
    pub password_hash: &'a str,
    pub affiliation: &'a str,
    pub name: &'a str,
    pub contact: &'a str,
    pub company: Option<&'a str>,
    /// 이미 JSON 직렬화된 프로필 (빈 프로필은 NULL)
    pub profile_json: Option<String>,
    pub agreed_at: NaiveDateTime,
    pub role: UserRole,
}

pub async fn create_user(db: impl MySqlExecutor<'_>, u: NewUser<'_>) -> AppResult<i64> {
    let result = sqlx::query(
        "INSERT INTO users
            (email, password_hash, affiliation, role, name, contact, company, profile,
             terms_agreed_at, third_party_agreed_at, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(u.email)
    .bind(u.password_hash)
    .bind(u.affiliation)
    .bind(u.role.value())
    .bind(u.name)
    .bind(u.contact)
    .bind(u.company)
    .bind(u.profile_json)
    .bind(u.agreed_at)
    .bind(u.agreed_at)
    .bind(u.agreed_at)
    .execute(db)
    .await?;
    Ok(result.last_insert_id() as i64)
}

pub async fn mark_email_verified(
    db: impl MySqlExecutor<'_>,
    id: i64,
    at: NaiveDateTime,
) -> AppResult<()> {
    sqlx::query("UPDATE users SET email_verified_at = ?, updated_at = ? WHERE id = ?")
        .bind(at)
        .bind(at)
        .bind(id)
        .execute(db)
        .await?;
    Ok(())
}

pub async fn update_last_login(
    db: impl MySqlExecutor<'_>,
    id: i64,
    at: NaiveDateTime,
) -> AppResult<()> {
    sqlx::query("UPDATE users SET last_login_at = ?, updated_at = ? WHERE id = ?")
        .bind(at)
        .bind(at)
        .bind(id)
        .execute(db)
        .await?;
    Ok(())
}

/// 목록 필터 (PHP buildListWhere 의 인자)
pub struct ListFilter<'a> {
    pub affiliation: &'a str,
    pub role: Option<i64>,
    pub is_active: Option<bool>,
    pub search: Option<&'a str>,
}

impl ListFilter<'_> {
    fn where_clause(&self) -> String {
        let mut conditions = vec!["affiliation = ?", "deleted_at IS NULL"];
        if self.role.is_some() {
            conditions.push("role = ?");
        }
        if self.is_active.is_some() {
            conditions.push("is_active = ?");
        }
        if self.search.is_some_and(|s| !s.is_empty()) {
            conditions.push("(email LIKE ? OR name LIKE ?)");
        }
        conditions.join(" AND ")
    }

    fn bind<'q, O>(
        &'q self,
        mut q: sqlx::query::QueryAs<'q, sqlx::MySql, O, sqlx::mysql::MySqlArguments>,
    ) -> sqlx::query::QueryAs<'q, sqlx::MySql, O, sqlx::mysql::MySqlArguments> {
        q = q.bind(self.affiliation);
        if let Some(role) = self.role {
            q = q.bind(role);
        }
        if let Some(active) = self.is_active {
            q = q.bind(i64::from(active));
        }
        if let Some(search) = self.search.filter(|s| !s.is_empty()) {
            let like = format!("%{search}%");
            q = q.bind(like.clone()).bind(like);
        }
        q
    }
}

pub async fn paginate_by_affiliation(
    db: impl MySqlExecutor<'_>,
    filter: &ListFilter<'_>,
    sort_column: &'static str,
    sort_direction: &'static str,
    limit: i64,
    offset: i64,
) -> AppResult<Vec<ListRow>> {
    // sort_column·sort_direction 은 UserListQuery 화이트리스트에서만 온다
    let sql = format!(
        "SELECT id, email, name, affiliation, role, is_active, email_verified_at, created_at FROM users
         WHERE {}
         ORDER BY {sort_column} {sort_direction}
         LIMIT ? OFFSET ?",
        filter.where_clause()
    );
    let q = filter
        .bind(sqlx::query_as::<_, ListRow>(&sql))
        .bind(limit)
        .bind(offset);
    Ok(q.fetch_all(db).await?)
}

pub async fn count_by_affiliation(
    db: impl MySqlExecutor<'_>,
    filter: &ListFilter<'_>,
) -> AppResult<i64> {
    let sql = format!("SELECT COUNT(*) FROM users WHERE {}", filter.where_clause());
    let (count,): (i64,) = filter
        .bind(sqlx::query_as::<_, (i64,)>(&sql))
        .fetch_one(db)
        .await?;
    Ok(count)
}

pub async fn find_manageable_by_id(
    db: impl MySqlExecutor<'_>,
    id: i64,
    affiliation: &str,
) -> AppResult<Option<ManageRow>> {
    Ok(sqlx::query_as::<_, ManageRow>(
        "SELECT id, email, name, affiliation, role, is_active, contact, company, profile, email_verified_at, last_login_at, created_at FROM users
         WHERE id = ? AND affiliation = ? AND deleted_at IS NULL
         LIMIT 1",
    )
    .bind(id)
    .bind(affiliation)
    .fetch_optional(db)
    .await?)
}

/// UPDATE 가능한 컬럼 값
pub enum FieldValue {
    Str(Option<String>),
    Int(i64),
}

/// 화이트리스트 컬럼만 부분 수정 (PHP updateFields)
pub async fn update_fields(
    db: impl MySqlExecutor<'_>,
    id: i64,
    fields: Vec<(&'static str, FieldValue)>,
    at: NaiveDateTime,
) -> AppResult<()> {
    const UPDATABLE: [&str; 6] = ["name", "contact", "company", "profile", "role", "is_active"];
    let fields: Vec<_> = fields
        .into_iter()
        .filter(|(c, _)| UPDATABLE.contains(c))
        .collect();
    if fields.is_empty() {
        return Ok(());
    }
    let mut sets: Vec<String> = fields.iter().map(|(c, _)| format!("{c} = ?")).collect();
    sets.push("updated_at = ?".into());
    let sql = format!("UPDATE users SET {} WHERE id = ?", sets.join(", "));
    let mut q = sqlx::query(&sql);
    for (_, value) in fields {
        q = match value {
            FieldValue::Str(s) => q.bind(s),
            FieldValue::Int(i) => q.bind(i),
        };
    }
    q.bind(at).bind(id).execute(db).await?;
    Ok(())
}

pub async fn update_password(
    db: impl MySqlExecutor<'_>,
    id: i64,
    hash: &str,
    at: NaiveDateTime,
) -> AppResult<()> {
    sqlx::query("UPDATE users SET password_hash = ?, updated_at = ? WHERE id = ?")
        .bind(hash)
        .bind(at)
        .bind(id)
        .execute(db)
        .await?;
    Ok(())
}

/// 소프트 삭제 + 비활성화
pub async fn soft_delete(db: impl MySqlExecutor<'_>, id: i64, at: NaiveDateTime) -> AppResult<()> {
    sqlx::query(
        "UPDATE users SET deleted_at = ?, is_active = 0, updated_at = ?
         WHERE id = ? AND deleted_at IS NULL",
    )
    .bind(at)
    .bind(at)
    .bind(id)
    .execute(db)
    .await?;
    Ok(())
}

// ───────────────────────────── refresh_tokens ─────────────────────────────

pub async fn store_refresh_token(
    db: impl MySqlExecutor<'_>,
    user_id: i64,
    hash: &str,
    expires_at: NaiveDateTime,
) -> AppResult<()> {
    sqlx::query("INSERT INTO refresh_tokens (user_id, token_hash, expires_at) VALUES (?, ?, ?)")
        .bind(user_id)
        .bind(hash)
        .bind(expires_at)
        .execute(db)
        .await?;
    Ok(())
}

pub async fn find_refresh_token(
    db: impl MySqlExecutor<'_>,
    hash: &str,
) -> AppResult<Option<TokenRow>> {
    Ok(sqlx::query_as::<_, TokenRow>(
        "SELECT id, user_id, expires_at, revoked_at AS closed_at FROM refresh_tokens
         WHERE token_hash = ?
         LIMIT 1",
    )
    .bind(hash)
    .fetch_optional(db)
    .await?)
}

/// find_refresh_token + 행 잠금(FOR UPDATE) — 트랜잭션 안에서만 사용 (PHP `lockByHash`)
pub async fn lock_refresh_token(
    db: impl MySqlExecutor<'_>,
    hash: &str,
) -> AppResult<Option<TokenRow>> {
    Ok(sqlx::query_as::<_, TokenRow>(
        "SELECT id, user_id, expires_at, revoked_at AS closed_at FROM refresh_tokens
         WHERE token_hash = ?
         LIMIT 1
         FOR UPDATE",
    )
    .bind(hash)
    .fetch_optional(db)
    .await?)
}

pub async fn revoke_refresh_token(
    db: impl MySqlExecutor<'_>,
    id: i64,
    at: NaiveDateTime,
) -> AppResult<()> {
    sqlx::query("UPDATE refresh_tokens SET revoked_at = ? WHERE id = ? AND revoked_at IS NULL")
        .bind(at)
        .bind(id)
        .execute(db)
        .await?;
    Ok(())
}

/// 비밀번호 변경·재설정 시 사용자 토큰 전체 삭제 — 폐기로 남기면 옛 기기의 refresh 가
/// 재사용 감지를 일으켜 새 세션까지 무효화된다 (PHP `deleteAllForUser`)
pub async fn delete_all_refresh_tokens(db: impl MySqlExecutor<'_>, user_id: i64) -> AppResult<()> {
    sqlx::query("DELETE FROM refresh_tokens WHERE user_id = ?")
        .bind(user_id)
        .execute(db)
        .await?;
    Ok(())
}

pub async fn revoke_all_refresh_tokens(
    db: impl MySqlExecutor<'_>,
    user_id: i64,
    at: NaiveDateTime,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE refresh_tokens SET revoked_at = ? WHERE user_id = ? AND revoked_at IS NULL",
    )
    .bind(at)
    .bind(user_id)
    .execute(db)
    .await?;
    Ok(())
}

// ───────────────────────────── email_verifications ─────────────────────────────

pub async fn store_verification(
    db: impl MySqlExecutor<'_>,
    user_id: i64,
    hash: &str,
    expires_at: NaiveDateTime,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO email_verifications (user_id, token_hash, expires_at) VALUES (?, ?, ?)",
    )
    .bind(user_id)
    .bind(hash)
    .bind(expires_at)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn find_verification(
    db: impl MySqlExecutor<'_>,
    hash: &str,
) -> AppResult<Option<TokenRow>> {
    Ok(sqlx::query_as::<_, TokenRow>(
        "SELECT id, user_id, expires_at, consumed_at AS closed_at FROM email_verifications
         WHERE token_hash = ?
         LIMIT 1",
    )
    .bind(hash)
    .fetch_optional(db)
    .await?)
}

pub async fn consume_verification(
    db: impl MySqlExecutor<'_>,
    id: i64,
    at: NaiveDateTime,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE email_verifications SET consumed_at = ? WHERE id = ? AND consumed_at IS NULL",
    )
    .bind(at)
    .bind(id)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn delete_unconsumed_verifications(
    db: impl MySqlExecutor<'_>,
    user_id: i64,
) -> AppResult<()> {
    sqlx::query("DELETE FROM email_verifications WHERE user_id = ? AND consumed_at IS NULL")
        .bind(user_id)
        .execute(db)
        .await?;
    Ok(())
}
