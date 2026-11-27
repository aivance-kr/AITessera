//! 도메인 모델·요청 DTO (PHP `App\Domain\*`).
//!
//! 요청 DTO 의 검증 순서와 메시지는 PHP(Respect\Validation) 와 동일해야 한다 —
//! 422 응답의 `message` 는 첫 번째 오류 메시지이기 때문이다.

use serde_json::Value;

use crate::error::{AppError, AppResult};
use crate::php::{self, Input};

// ───────────────────────────── 열거형 ─────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Affiliation {
    Aicura,
    Aicopia,
    Aicreo,
    Aivance,
    Ailicet,
}

pub const AFFILIATIONS: [&str; 5] = ["aicura", "aicopia", "aicreo", "aivance", "ailicet"];

impl Affiliation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Aicura => "aicura",
            Self::Aicopia => "aicopia",
            Self::Aicreo => "aicreo",
            Self::Aivance => "aivance",
            Self::Ailicet => "ailicet",
        }
    }

    pub fn try_from_str(s: &str) -> Option<Self> {
        Some(match s {
            "aicura" => Self::Aicura,
            "aicopia" => Self::Aicopia,
            "aicreo" => Self::Aicreo,
            "aivance" => Self::Aivance,
            "ailicet" => Self::Ailicet,
            _ => return None,
        })
    }

    /// DB 값 — 알 수 없는 값은 PHP `Affiliation::from()` 처럼 오류
    pub fn from_db(s: &str) -> AppResult<Self> {
        Self::try_from_str(s).ok_or_else(|| {
            AppError::Internal(format!(
                "\"{s}\" is not a valid backing value for enum App\\Domain\\Affiliation"
            ))
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum UserRole {
    Operator = 1,
    Agency = 2,
    Member = 3,
}

impl UserRole {
    pub fn value(self) -> i64 {
        self as i64
    }

    pub fn try_from_int(v: i64) -> Option<Self> {
        Some(match v {
            1 => Self::Operator,
            2 => Self::Agency,
            3 => Self::Member,
            _ => return None,
        })
    }

    pub fn from_db(v: i64) -> AppResult<Self> {
        Self::try_from_int(v).ok_or_else(|| {
            AppError::Internal(format!(
                "{v} is not a valid backing value for enum App\\Domain\\UserRole"
            ))
        })
    }
}

// ───────────────────────────── 비밀번호 정책 ─────────────────────────────

pub const PASSWORD_MIN_LENGTH: usize = 10;

pub fn password_policy_ok(v: &Value) -> bool {
    let Value::String(s) = v else { return false };
    // Respect Length 는 mb_strlen 기준(문자 수)
    s.chars().count() >= PASSWORD_MIN_LENGTH
        && s.bytes().any(|b| b.is_ascii_alphabetic())
        && s.bytes().any(|b| b.is_ascii_digit())
        && s.chars().any(|c| !c.is_ascii_alphanumeric())
}

pub fn password_policy_describe() -> String {
    format!(
        "비밀번호는 {PASSWORD_MIN_LENGTH}자 이상이며 영문자·숫자·특수문자를 각각 1개 이상 포함해야 합니다."
    )
}

// ───────────────────────────── 프로필 스키마 ─────────────────────────────

type Rule = fn(&Value) -> bool;

fn profile_fields(aff: Affiliation) -> &'static [(&'static str, Rule, &'static str)] {
    fn positive_int(v: &Value) -> bool {
        php::is_int(v).is_some_and(|i| i > 0)
    }
    fn string_type(v: &Value) -> bool {
        v.is_string()
    }
    match aff {
        Affiliation::Aicura => &[
            ("age", positive_int, "age 는 양의 정수여야 합니다."),
            ("sex", string_type, "sex 는 문자열이어야 합니다."),
            (
                "where_from",
                string_type,
                "where_from 는 문자열이어야 합니다.",
            ),
        ],
        Affiliation::Aicopia => &[
            ("gender", string_type, "gender 는 문자열이어야 합니다."),
            (
                "birthday",
                php::valid_ymd,
                "birthday 는 YYYY-MM-DD 형식이어야 합니다.",
            ),
            ("zipcode", string_type, "zipcode 는 문자열이어야 합니다."),
            ("address1", string_type, "address1 는 문자열이어야 합니다."),
            ("address2", string_type, "address2 는 문자열이어야 합니다."),
        ],
        Affiliation::Aicreo | Affiliation::Aivance | Affiliation::Ailicet => &[],
    }
}

/// PHP `ProfileSchema::validate()` — 허용되지 않은 키 오류 → 필드 규칙 오류 순
pub fn validate_profile(aff: Affiliation, profile: &Value) -> Vec<String> {
    let fields = profile_fields(aff);
    let mut errors = Vec::new();
    match profile {
        Value::Object(map) => {
            for key in map.keys() {
                if !fields.iter().any(|(name, _, _)| name == key) {
                    errors.push(format!(
                        "'{key}' 은(는) {} 소속에서 허용되지 않는 프로필 항목입니다.",
                        aff.as_str()
                    ));
                }
            }
            for (name, rule, message) in fields {
                if let Some(v) = map.get(*name) {
                    if !rule(v) {
                        errors.push((*message).to_string());
                    }
                }
            }
        }
        // JSON 배열(리스트)은 PHP 에서 정수 키 배열 — 모든 키가 허용되지 않은 항목
        Value::Array(list) => {
            for i in 0..list.len() {
                errors.push(format!(
                    "'{i}' 은(는) {} 소속에서 허용되지 않는 프로필 항목입니다.",
                    aff.as_str()
                ));
            }
        }
        _ => {}
    }
    errors
}

// ─────────────────────── Respect\Validation 키 규칙 재현 ───────────────────────

/// 하나의 검사 규칙 — (통과 여부, 실패 메시지 꼬리)
struct Check {
    pass: bool,
    message: String,
}

/// `v::key(name, chain, mandatory)` 하나를 평가한다.
/// Respect 는 키마다 실패한 규칙 중 **마지막** 규칙의 메시지를 낸다.
fn key_rule(
    errors: &mut Vec<String>,
    data: &Input,
    name: &str,
    mandatory: bool,
    optional_wrapper: bool,
    checks: impl FnOnce(&Value) -> Vec<Check>,
) {
    let Some(value) = data.get(name) else {
        if mandatory {
            errors.push(format!("{name} must be present"));
        }
        return;
    };
    // v::optional() — null 또는 '' 은 통과
    if optional_wrapper && (value.is_null() || value.as_str() == Some("")) {
        return;
    }
    if let Some(last_fail) = checks(value).into_iter().rev().find(|c| !c.pass) {
        errors.push(format!("{name} {}", last_fail.message));
    }
}

fn c(pass: bool, message: &str) -> Check {
    Check {
        pass,
        message: message.to_string(),
    }
}

const IN_AFFILIATIONS: &str =
    "must be in `{ \"aicura\", \"aicopia\", \"aicreo\", \"aivance\", \"ailicet\" }`";

fn email_rule(errors: &mut Vec<String>, data: &Input) {
    key_rule(errors, data, "email", true, false, |v| {
        vec![
            c(v.is_string(), "must be of type string"),
            c(
                v.as_str().is_some_and(php::valid_email),
                "must be valid email",
            ),
        ]
    });
}

fn affiliation_rule(errors: &mut Vec<String>, data: &Input) {
    key_rule(errors, data, "affiliation", true, false, |v| {
        vec![c(php::loose_in_str(v, &AFFILIATIONS), IN_AFFILIATIONS)]
    });
}

fn non_empty_string_rule(errors: &mut Vec<String>, data: &Input, name: &str) {
    key_rule(errors, data, name, true, false, |v| {
        vec![
            c(v.is_string(), "must be of type string"),
            c(php::not_empty(v), "must not be empty"),
        ]
    });
}

fn optional_string_rule(errors: &mut Vec<String>, data: &Input, name: &str) {
    key_rule(errors, data, name, false, true, |v| {
        vec![c(v.is_string(), "must be of type string")]
    });
}

fn true_val_rule(errors: &mut Vec<String>, data: &Input, name: &str) {
    key_rule(errors, data, name, true, false, |v| {
        vec![c(php::true_val(v), "must evaluate to `true`")]
    });
}

/// `(string) $data[$key]` — 검증을 통과한 스칼라 값의 문자열화
fn to_php_string(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(true)) => "1".into(),
        _ => String::new(),
    }
}

fn company_of(data: &Input) -> Option<String> {
    php::non_empty_str(data.get("company")).map(str::to_string)
}

/// `$data['profile'] ?? []` 를 소속 스키마로 검증 (소속이 유효할 때만)
fn profile_of(errors: &mut Vec<String>, data: &Input) -> Value {
    let profile = match data.get("profile") {
        None | Some(Value::Null) => Value::Array(vec![]),
        Some(v) => v.clone(),
    };
    if let Some(aff) = data
        .get("affiliation")
        .and_then(Value::as_str)
        .and_then(Affiliation::try_from_str)
    {
        if profile.is_object() || profile.is_array() {
            errors.extend(validate_profile(aff, &profile));
        } else {
            errors.push("profile 은(는) 객체 형식이어야 합니다.".into());
        }
    }
    if profile.is_object() || profile.is_array() {
        profile
    } else {
        Value::Array(vec![])
    }
}

fn password_rule(errors: &mut Vec<String>, data: &Input) {
    let ok = data.get("password").is_some_and(password_policy_ok);
    if !ok {
        errors.push(password_policy_describe());
    }
}

// ───────────────────────────── 요청 DTO ─────────────────────────────

pub struct LoginRequest {
    pub email: String,
    pub password: String,
    pub affiliation: Affiliation,
}

impl LoginRequest {
    pub fn from_input(data: &Input) -> AppResult<Self> {
        let mut errors = Vec::new();
        email_rule(&mut errors, data);
        non_empty_string_rule(&mut errors, data, "password");
        affiliation_rule(&mut errors, data);
        if !errors.is_empty() {
            return Err(AppError::validation(errors));
        }
        Ok(Self {
            email: to_php_string(data.get("email")),
            password: to_php_string(data.get("password")),
            affiliation: affiliation_after_check(data)?,
        })
    }
}

/// `Affiliation::from((string) $data['affiliation'])` — `in` 느슨한 비교를 통과한 bool(true) 등은
/// PHP 에서 ValueError(500) 가 되므로 동일하게 내부 오류로 처리한다.
fn affiliation_after_check(data: &Input) -> AppResult<Affiliation> {
    Affiliation::from_db(&to_php_string(data.get("affiliation")))
}

pub struct RegisterRequest {
    pub email: String,
    pub password: String,
    pub affiliation: Affiliation,
    pub name: String,
    pub contact: String,
    pub company: Option<String>,
    pub profile: Value,
}

impl RegisterRequest {
    pub fn from_input(data: &Input) -> AppResult<Self> {
        let mut errors = Vec::new();
        email_rule(&mut errors, data);
        affiliation_rule(&mut errors, data);
        non_empty_string_rule(&mut errors, data, "name");
        non_empty_string_rule(&mut errors, data, "contact");
        optional_string_rule(&mut errors, data, "company");
        true_val_rule(&mut errors, data, "terms_agreed");
        true_val_rule(&mut errors, data, "third_party_agreed");
        password_rule(&mut errors, data);
        let profile = profile_of(&mut errors, data);
        if !errors.is_empty() {
            return Err(AppError::validation(errors));
        }
        Ok(Self {
            email: to_php_string(data.get("email")),
            password: to_php_string(data.get("password")),
            affiliation: affiliation_after_check(data)?,
            name: to_php_string(data.get("name")),
            contact: to_php_string(data.get("contact")),
            company: company_of(data),
            profile,
        })
    }
}

pub struct CreateOperatorRequest {
    pub email: String,
    pub password: String,
    pub role: UserRole,
    pub affiliation: Affiliation,
    pub name: String,
    pub contact: String,
    pub company: Option<String>,
    pub profile: Value,
}

impl CreateOperatorRequest {
    pub fn from_input(data: &Input) -> AppResult<Self> {
        let mut errors = Vec::new();
        email_rule(&mut errors, data);
        key_rule(&mut errors, data, "role", true, false, |v| {
            vec![
                c(php::int_val(v), "must be an integer number"),
                c(php::loose_in_int(v, &[1, 2, 3]), "must be in `{ 1, 2, 3 }`"),
            ]
        });
        affiliation_rule(&mut errors, data);
        non_empty_string_rule(&mut errors, data, "name");
        non_empty_string_rule(&mut errors, data, "contact");
        optional_string_rule(&mut errors, data, "company");
        password_rule(&mut errors, data);
        let profile = profile_of(&mut errors, data);
        if !errors.is_empty() {
            return Err(AppError::validation(errors));
        }
        let role_int = data.get("role").and_then(php::numeric_to_int).unwrap_or(0);
        Ok(Self {
            email: to_php_string(data.get("email")),
            password: to_php_string(data.get("password")),
            role: UserRole::from_db(role_int)?,
            affiliation: affiliation_after_check(data)?,
            name: to_php_string(data.get("name")),
            contact: to_php_string(data.get("contact")),
            company: company_of(data),
            profile,
        })
    }
}

pub const LOG_LEVELS: [&str; 6] = ["debug", "info", "notice", "warning", "error", "critical"];

pub struct LogRequest {
    pub level: String,
    pub message: String,
    pub context: Value,
    pub source: Option<String>,
    pub user_id: Option<i64>,
    pub logged_at: Option<String>,
}

impl LogRequest {
    pub fn from_input(data: &Input) -> AppResult<Self> {
        let mut errors = Vec::new();
        key_rule(&mut errors, data, "level", true, false, |v| {
            vec![c(
                php::loose_in_str(v, &LOG_LEVELS),
                "must be in `{ \"debug\", \"info\", \"notice\", \"warning\", \"error\", ... }`",
            )]
        });
        non_empty_string_rule(&mut errors, data, "message");
        key_rule(&mut errors, data, "context", false, true, |v| {
            vec![c(v.is_array() || v.is_object(), "must be of type array")]
        });
        optional_string_rule(&mut errors, data, "source");
        key_rule(&mut errors, data, "user_id", false, true, |v| {
            vec![
                c(php::is_int(v).is_some(), "must be of type integer"),
                c(
                    php::numeric_f64(v).is_some_and(|f| f > 0.0),
                    "must be positive",
                ),
            ]
        });
        key_rule(&mut errors, data, "logged_at", false, true, |v| {
            vec![c(php::strtotime_ok(v), "must be a valid date/time")]
        });
        if !errors.is_empty() {
            return Err(AppError::validation(errors));
        }
        let context = match data.get("context") {
            Some(v @ (Value::Array(_) | Value::Object(_))) => v.clone(),
            _ => Value::Array(vec![]),
        };
        Ok(Self {
            level: to_php_string(data.get("level")),
            message: to_php_string(data.get("message")),
            context,
            source: php::non_empty_str(data.get("source")).map(str::to_string),
            user_id: data.get("user_id").and_then(php::is_int),
            logged_at: php::non_empty_str(data.get("logged_at")).map(str::to_string),
        })
    }

    /// 큐 메시지 본문 — 원본을 빌린 채 직렬화한다 (context 는 PHP 정규화 규칙 적용)
    pub fn to_payload(&self) -> LogPayload<'_> {
        LogPayload {
            level: &self.level,
            message: &self.message,
            context: php::PhpNormalized(&self.context),
            source: self.source.as_deref(),
            user_id: self.user_id,
            logged_at: self.logged_at.as_deref(),
        }
    }
}

/// `log_queue` 메시지 — 필드 순서 = PHP 배열 키 순서
#[derive(serde::Serialize)]
pub struct LogPayload<'a> {
    level: &'a str,
    message: &'a str,
    context: php::PhpNormalized<'a>,
    source: Option<&'a str>,
    user_id: Option<i64>,
    logged_at: Option<&'a str>,
}

/// PATCH 공통 필드 파싱 (UpdateMeRequest / UpdateUserRequest 공유 부분)
#[derive(Default)]
pub struct PatchCommon {
    pub provided: Vec<&'static str>,
    pub name: Option<String>,
    pub contact: Option<String>,
    pub company: Option<String>,
    pub profile: Value,
}

impl PatchCommon {
    fn parse(data: &Input, errors: &mut Vec<String>) -> Self {
        let mut out = Self {
            profile: Value::Array(vec![]),
            ..Default::default()
        };
        for (field, slot) in [("name", 0u8), ("contact", 1u8)] {
            if let Some(v) = data.get(field) {
                out.provided.push(field);
                match v {
                    Value::String(s) if !php_trim(s).is_empty() => {
                        if slot == 0 {
                            out.name = Some(s.clone())
                        } else {
                            out.contact = Some(s.clone())
                        }
                    }
                    _ => errors.push(format!(
                        "{field} 은(는) 비어 있지 않은 문자열이어야 합니다."
                    )),
                }
            }
        }
        if let Some(raw) = data.get("company") {
            out.provided.push("company");
            match raw {
                Value::Null => {}
                Value::String(s) if s.is_empty() => {}
                Value::String(s) => out.company = Some(s.clone()),
                _ => errors.push("company 은(는) 문자열이거나 null 이어야 합니다.".into()),
            }
        }
        if let Some(raw) = data.get("profile") {
            out.provided.push("profile");
            match raw {
                Value::Object(_) | Value::Array(_) => out.profile = raw.clone(),
                _ => errors.push("profile 은(는) 객체 형식이어야 합니다.".into()),
            }
        }
        out
    }

    pub fn has(&self, field: &str) -> bool {
        self.provided.contains(&field)
    }
}

pub struct UpdateMeRequest {
    pub common: PatchCommon,
    pub password: Option<String>,
    pub current_password: Option<String>,
}

impl UpdateMeRequest {
    pub fn from_input(data: &Input) -> AppResult<Self> {
        let mut errors = Vec::new();
        let mut common = PatchCommon::parse(data, &mut errors);
        let mut password = None;
        let mut current_password = None;
        if let Some(v) = data.get("password") {
            common.provided.push("password");
            if password_policy_ok(v) {
                password = v.as_str().map(str::to_string);
            } else {
                errors.push(password_policy_describe());
            }
            match php::non_empty_str(data.get("current_password")) {
                Some(cur) => current_password = Some(cur.to_string()),
                None => {
                    errors.push("current_password 는 비밀번호 변경 시 반드시 필요합니다.".into())
                }
            }
        }
        if common.provided.is_empty() {
            errors.push("수정할 필드가 없습니다.".into());
        }
        if !errors.is_empty() {
            return Err(AppError::validation(errors));
        }
        Ok(Self {
            common,
            password,
            current_password,
        })
    }
}

pub struct UpdateUserRequest {
    pub common: PatchCommon,
    pub role: Option<UserRole>,
    pub is_active: Option<bool>,
    pub password: Option<String>,
}

impl UpdateUserRequest {
    pub fn from_input(data: &Input) -> AppResult<Self> {
        let mut errors = Vec::new();
        let mut common = PatchCommon::parse(data, &mut errors);
        let mut role = None;
        if let Some(raw) = data.get("role") {
            common.provided.push("role");
            match php::numeric_to_int(raw).and_then(UserRole::try_from_int) {
                Some(r) => role = Some(r),
                None => errors.push("role 값이 올바르지 않습니다.".into()),
            }
        }
        let mut is_active = None;
        if let Some(raw) = data.get("is_active") {
            common.provided.push("is_active");
            is_active = parse_bool(raw);
            if is_active.is_none() {
                errors.push("is_active 값이 올바르지 않습니다.".into());
            }
        }
        let mut password = None;
        if let Some(v) = data.get("password") {
            common.provided.push("password");
            if password_policy_ok(v) {
                password = v.as_str().map(str::to_string);
            } else {
                errors.push(password_policy_describe());
            }
        }
        if common.provided.is_empty() {
            errors.push("수정할 필드가 없습니다.".into());
        }
        if !errors.is_empty() {
            return Err(AppError::validation(errors));
        }
        Ok(Self {
            common,
            role,
            is_active,
            password,
        })
    }
}

/// PHP `Booleans::parse()`
pub fn parse_bool(v: &Value) -> Option<bool> {
    match v {
        Value::Bool(b) => Some(*b),
        Value::String(s) => match s.as_str() {
            "1" | "true" => Some(true),
            "0" | "false" => Some(false),
            _ => None,
        },
        Value::Number(n) if n.is_i64() || n.is_u64() => match n.as_i64() {
            Some(1) => Some(true),
            Some(0) => Some(false),
            _ => None,
        },
        _ => None,
    }
}

pub const DEFAULT_PER_PAGE: i64 = 20;
pub const MAX_PER_PAGE: i64 = 100;

pub struct UserListQuery {
    pub page: i64,
    pub per_page: i64,
    pub role: Option<UserRole>,
    pub is_active: Option<bool>,
    pub search: Option<String>,
    pub sort_column: &'static str,
    pub sort_direction: &'static str,
}

impl UserListQuery {
    /// 쿼리스트링 파라미터 — 값은 문자열, `key[]` 형태는 배열(비문자열)로 취급
    pub fn from_params(params: &Input) -> AppResult<Self> {
        let mut errors = Vec::new();
        let positive = |v: Option<&Value>, default: i64| -> i64 {
            v.and_then(php::numeric_to_int)
                .filter(|i| *i >= 1)
                .unwrap_or(default)
        };
        let page = positive(params.get("page"), 1);
        let per_page = positive(params.get("per_page"), DEFAULT_PER_PAGE).min(MAX_PER_PAGE);

        let is_set = |k: &str| {
            params
                .get(k)
                .is_some_and(|v| !v.is_null() && v.as_str() != Some(""))
        };

        let mut role = None;
        if is_set("role") {
            match params
                .get("role")
                .and_then(php::numeric_to_int)
                .and_then(UserRole::try_from_int)
            {
                Some(r) => role = Some(r),
                None => errors.push("role 필터 값이 올바르지 않습니다.".into()),
            }
        }
        let mut is_active = None;
        if is_set("is_active") {
            is_active = params.get("is_active").and_then(parse_bool);
            if is_active.is_none() {
                errors.push("is_active 필터 값이 올바르지 않습니다.".into());
            }
        }
        let search = params
            .get("q")
            .and_then(Value::as_str)
            .map(php_trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);

        let mut sort_column = "created_at";
        let mut sort_direction = "DESC";
        if let Some(raw) = params
            .get("sort")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        {
            let (direction, field) = match raw.strip_prefix('-') {
                Some(rest) => ("DESC", rest),
                None => ("ASC", raw),
            };
            let column = match field {
                "created_at" => Some("created_at"),
                "id" => Some("id"),
                "email" => Some("email"),
                "name" => Some("name"),
                _ => None,
            };
            match column {
                Some(col) => {
                    sort_column = col;
                    sort_direction = direction;
                }
                None => errors.push("sort 필드가 올바르지 않습니다.".into()),
            }
        }
        if !errors.is_empty() {
            return Err(AppError::validation(errors));
        }
        Ok(Self {
            page,
            per_page,
            role,
            is_active,
            search,
            sort_column,
            sort_direction,
        })
    }

    pub fn offset(&self) -> i64 {
        (self.page - 1) * self.per_page
    }
}

/// PHP `trim()` 기본 문자 집합
pub fn php_trim(s: &str) -> &str {
    s.trim_matches(|c: char| matches!(c, ' ' | '\t' | '\n' | '\r' | '\0' | '\x0B'))
}
