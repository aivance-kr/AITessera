//! PHP 런타임 의미론 재현 헬퍼.
//!
//! 기존 PHP 구현과 입력 해석·검증 결과가 1:1 로 같아야 교체가 가능하므로,
//! `is_numeric`, `(int)` 캐스트, `empty()`, `filter_var(FILTER_VALIDATE_EMAIL)` 등
//! 응답에 영향을 주는 PHP 동작을 여기서 흉내 낸다.

use std::sync::LazyLock;

use serde_json::{Map, Value};

/// 요청 입력 — PHP 연관 배열에 해당 (키 순서 보존)
pub type Input = Map<String, Value>;

/// PHP `is_string($v) && $v !== ''`
pub fn non_empty_str(v: Option<&Value>) -> Option<&str> {
    match v {
        Some(Value::String(s)) if !s.is_empty() => Some(s.as_str()),
        _ => None,
    }
}

/// PHP `is_numeric()` — 앞뒤 공백 허용(PHP 8), 정수/실수/지수 표기 허용
pub fn is_numeric(v: &Value) -> bool {
    match v {
        Value::Number(_) => true,
        Value::String(s) => numeric_str(s).is_some(),
        _ => false,
    }
}

fn numeric_str(s: &str) -> Option<f64> {
    let t = s.trim_matches(|c: char| matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0B' | '\x0C'));
    if t.is_empty() {
        return None;
    }
    static RE: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"^[+-]?(\d+(\.\d*)?|\.\d+)([eE][+-]?\d+)?$").expect("numeric regex")
    });
    if !RE.is_match(t) {
        return None;
    }
    t.parse::<f64>().ok()
}

/// `is_numeric($v) ? (float) $v : null`
pub fn numeric_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => numeric_str(s),
        _ => None,
    }
}

/// `is_numeric($v) ? (int) $v : null` — 실수는 0 방향 절삭, 범위 밖은 포화
pub fn numeric_to_int(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(f64_to_int)),
        Value::String(s) => {
            let f = numeric_str(s)?;
            // 정수 문자열은 정밀도 손실 없이 처리
            match s.trim().trim_start_matches('+').parse::<i64>() {
                Ok(i) => Some(i),
                Err(_) => Some(f64_to_int(f)),
            }
        }
        _ => None,
    }
}

fn f64_to_int(f: f64) -> i64 {
    if f.is_nan() {
        0
    } else {
        f.trunc().clamp(i64::MIN as f64, i64::MAX as f64) as i64
    }
}

/// Respect `NotEmpty` — 문자열은 trim 후 PHP `empty()` 판정
pub fn not_empty(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => {
            let t = crate::domain::php_trim(s);
            !(t.is_empty() || t == "0")
        }
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// Respect `TrueVal` — filter_var(FILTER_VALIDATE_BOOLEAN, NULL_ON_FAILURE) === true
pub fn true_val(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_i64() == Some(1) || n.as_f64() == Some(1.0),
        Value::String(s) => matches!(
            s.trim().to_lowercase().as_str(),
            "1" | "true" | "on" | "yes"
        ),
        _ => false,
    }
}

/// Respect `IntVal` — 정수 또는 `^-?\d+$` 문자열
pub fn int_val(v: &Value) -> bool {
    match v {
        Value::Number(n) => n.is_i64() || n.is_u64(),
        Value::String(s) => {
            let b = s.strip_prefix('-').unwrap_or(s);
            !b.is_empty() && b.bytes().all(|c| c.is_ascii_digit())
        }
        _ => false,
    }
}

/// PHP `is_int()` — JSON 정수만 (1.0 은 float)
pub fn is_int(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) if n.is_i64() => n.as_i64(),
        _ => None,
    }
}

/// PHP `in_array($v, $haystack)` (느슨한 비교) — 문자열 목록 대상
pub fn loose_in_str(v: &Value, haystack: &[&str]) -> bool {
    match v {
        Value::String(s) => haystack.contains(&s.as_str()),
        // 숫자·bool 과 비숫자 문자열의 느슨한 비교는 PHP 8 에서 false (true == "x" 만 예외)
        Value::Bool(true) => !haystack.is_empty(),
        _ => false,
    }
}

/// PHP `in_array($v, [1,2,3])` (느슨한 비교) — 정수 목록 대상
pub fn loose_in_int(v: &Value, haystack: &[i64]) -> bool {
    match v {
        Value::Number(n) => n
            .as_f64()
            .is_some_and(|f| haystack.iter().any(|h| *h as f64 == f)),
        Value::String(s) => numeric_str(s).is_some_and(|f| haystack.iter().any(|h| *h as f64 == f)),
        Value::Bool(b) => haystack.iter().any(|h| (*h != 0) == *b),
        _ => false,
    }
}

/// PHP `json_decode` 후 `json_encode` 와 같은 정규화 —
/// 빈 객체 `{}` 는 PHP 배열 `[]` 로 바뀌어 다시 `[]` 로 인코딩된다.
pub fn php_normalize(v: Value) -> Value {
    match v {
        Value::Object(m) if m.is_empty() => Value::Array(vec![]),
        Value::Object(m) => {
            Value::Object(m.into_iter().map(|(k, v)| (k, php_normalize(v))).collect())
        }
        Value::Array(a) => Value::Array(a.into_iter().map(php_normalize).collect()),
        other => other,
    }
}

/// `php_normalize` 의 복제 없는 직렬화 버전 — 빈 객체 `{}` 를 `[]` 로 쓰면서 원본을 빌린 채 직렬화한다
pub struct PhpNormalized<'a>(pub &'a Value);

impl serde::Serialize for PhpNormalized<'_> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::{SerializeMap, SerializeSeq};
        match self.0 {
            Value::Object(m) if m.is_empty() => s.serialize_seq(Some(0))?.end(),
            Value::Object(m) => {
                let mut map = s.serialize_map(Some(m.len()))?;
                for (k, v) in m {
                    map.serialize_entry(k, &PhpNormalized(v))?;
                }
                map.end()
            }
            Value::Array(a) => {
                let mut seq = s.serialize_seq(Some(a.len()))?;
                for v in a {
                    seq.serialize_element(&PhpNormalized(v))?;
                }
                seq.end()
            }
            other => other.serialize(s),
        }
    }
}

/// PHP 배열로서 "비었는가" (`$profile === []`)
pub fn is_empty_array(v: &Value) -> bool {
    match v {
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.is_empty(),
        _ => false,
    }
}

/// PHP `json_encode($v, JSON_UNESCAPED_UNICODE)` 상당 — 슬래시는 `\/` 로 이스케이프
pub fn json_encode_unescaped_unicode(v: &Value) -> String {
    serde_json::to_string(&PhpNormalized(v))
        .unwrap_or_else(|_| "null".into())
        .replace('/', "\\/")
}

/// `filter_var($s, FILTER_VALIDATE_EMAIL)` — php-src 의 정규식을 그대로 옮겼다.
pub fn valid_email(s: &str) -> bool {
    if s.len() > 320 {
        return false;
    }
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
        fancy_regex::Regex::new(concat!(
            r"(?i)^(?!(?:(?:\x22?\x5C[\x00-\x7E]\x22?)|(?:\x22?[^\x5C\x22]\x22?)){255,})",
            r"(?!(?:(?:\x22?\x5C[\x00-\x7E]\x22?)|(?:\x22?[^\x5C\x22]\x22?)){65,}@)",
            r"(?:(?:[\x21\x23-\x27\x2A\x2B\x2D\x2F-\x39\x3D\x3F\x5E-\x7E]+)|(?:\x22(?:[\x01-\x08\x0B\x0C\x0E-\x1F\x21\x23-\x5B\x5D-\x7F]|(?:\x5C[\x00-\x7F]))*\x22))",
            r"(?:\.(?:(?:[\x21\x23-\x27\x2A\x2B\x2D\x2F-\x39\x3D\x3F\x5E-\x7E]+)|(?:\x22(?:[\x01-\x08\x0B\x0C\x0E-\x1F\x21\x23-\x5B\x5D-\x7F]|(?:\x5C[\x00-\x7F]))*\x22)))*",
            r"@(?:(?:(?!.*[^.]{64,})(?:(?:(?:xn--)?[a-z0-9]+(?:-+[a-z0-9]+)*\.){1,126}){1,}(?:(?:[a-z][a-z0-9]*)|(?:(?:xn--)[a-z0-9]+))(?:-+[a-z0-9]+)*)",
            r"|(?:\[(?:(?:IPv6:(?:(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){7})|(?:(?!(?:.*[a-f0-9][:\]]){7,})(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){0,5})?::(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){0,5})?)))",
            r"|(?:(?:IPv6:(?:(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){5}:)|(?:(?!(?:.*[a-f0-9]:){5,})(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){0,3})?::(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){0,3}:)?)))?",
            r"(?:(?:25[0-5])|(?:2[0-4][0-9])|(?:1[0-9]{2})|(?:[1-9]?[0-9]))(?:\.(?:(?:25[0-5])|(?:2[0-4][0-9])|(?:1[0-9]{2})|(?:[1-9]?[0-9]))){3}))\]))\z",
        ))
        .expect("email regex")
    });
    RE.is_match(s).unwrap_or(false)
}

/// Respect `DateTime()` (형식 미지정) ≈ `strtotime($v) !== false`.
/// strtotime 의 자연어 표현 전부를 재현하지는 않고, 클라이언트가 실제로 보내는
/// ISO-8601 / `Y-m-d H:i:s` / 날짜 단독 / 유닉스 `@ts` 형식을 허용한다.
pub fn strtotime_ok(v: &Value) -> bool {
    let s = match v {
        Value::String(s) => s.trim(),
        // 숫자는 문자열로 캐스트되며 단순 숫자는 strtotime 이 거부한다
        _ => return false,
    };
    if s.is_empty() {
        return false;
    }
    if matches!(
        s.to_lowercase().as_str(),
        "now" | "today" | "tomorrow" | "yesterday" | "midnight" | "noon"
    ) {
        return true;
    }
    if let Some(ts) = s.strip_prefix('@') {
        return ts.parse::<i64>().is_ok();
    }
    use chrono::{DateTime, NaiveDate, NaiveDateTime};
    if DateTime::parse_from_rfc3339(s).is_ok() || DateTime::parse_from_rfc2822(s).is_ok() {
        return true;
    }
    const DT_FORMATS: [&str; 6] = [
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d %H:%M",
        "%Y/%m/%d %H:%M:%S",
    ];
    if DT_FORMATS
        .iter()
        .any(|f| NaiveDateTime::parse_from_str(s, f).is_ok())
    {
        return true;
    }
    const TZ_FORMATS: [&str; 4] = [
        "%Y-%m-%d %H:%M:%S%z",
        "%Y-%m-%dT%H:%M:%S%z",
        "%Y-%m-%d %H:%M:%S%.f%z",
        "%Y-%m-%dT%H:%M:%S%.f%z",
    ];
    if TZ_FORMATS
        .iter()
        .any(|f| DateTime::parse_from_str(s, f).is_ok())
    {
        return true;
    }
    ["%Y-%m-%d", "%Y/%m/%d"]
        .iter()
        .any(|f| NaiveDate::parse_from_str(s, f).is_ok())
}

/// Respect `Date('Y-m-d')` — 형식과 달력상 유효성 모두 검사
pub fn valid_ymd(v: &Value) -> bool {
    let s = match v {
        Value::String(s) => s.as_str(),
        Value::Number(n) => {
            return n.to_string().len() == 10 && valid_ymd(&Value::String(n.to_string()))
        }
        _ => return false,
    };
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn numeric_semantics() {
        assert!(is_numeric(&json!(" 12")));
        assert!(is_numeric(&json!("1e3")));
        assert!(!is_numeric(&json!("12abc")));
        assert!(!is_numeric(&json!(true)));
        assert_eq!(numeric_to_int(&json!("2.7")), Some(2));
        assert_eq!(numeric_to_int(&json!("1e2")), Some(100));
    }

    #[test]
    fn email_semantics() {
        assert!(valid_email("a@b.com"));
        assert!(valid_email("first.last+tag@sub.example.co.kr"));
        assert!(!valid_email("x"));
        assert!(!valid_email("a@b"));
        assert!(!valid_email("a..b@c.com"));
        assert!(!valid_email("a@-b.com"));
    }

    /// PHP 8.5 `filter_var(FILTER_VALIDATE_EMAIL)` 실측 결과와 대조
    #[test]
    fn email_parity_with_php() {
        let cases = [
            (true, "a@b.com"),
            (false, "a@b"),
            (true, "a.b@c.d.e"),
            (true, "\"quoted\"@example.com"),
            (false, "a..b@c.com"),
            (false, ".a@c.com"),
            (false, "a.@c.com"),
            (false, "a@-b.com"),
            (false, "a@b-.com"),
            (true, "a@b.c0m"),
            (false, "a@b.1com"),
            (true, "a+tag@example.co.kr"),
            (true, "user@[127.0.0.1]"),
            (true, "user@[IPv6:2001:db8::1]"),
            (false, "user@localhost"),
            (false, "한글@example.com"),
            (false, "user@exam_ple.com"),
            (true, "user@xn--bcher-kva.ch"),
            (true, "a@b.c"),
            (false, "user name@example.com"),
            (
                false,
                "abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmn@x.com",
            ),
            (
                false,
                "abc@aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.com",
            ),
            (true, "A@B.COM"),
            (false, "a@b.com."),
            (false, "test@sub.-domain.com"),
            (true, "x@y.z1"),
        ];
        for (expected, email) in cases {
            assert_eq!(valid_email(email), expected, "{email}");
        }
    }

    #[test]
    fn not_empty_semantics() {
        assert!(!not_empty(&json!("  ")));
        assert!(!not_empty(&json!("0")));
        assert!(not_empty(&json!(3)));
    }
}
