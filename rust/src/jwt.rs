//! JWT 발급·검증·JWKS (PHP `JwtIssuer` / `JwtAuthMiddleware` / `Jwks`).
//!
//! 서명·검증 암호 연산은 `jsonwebtoken` 라이브러리에 위임하고 허용 알고리즘을 1개로 고정한다.
//! PHP(lcobucci) 가 발급한 토큰은 iat/nbf/exp 가 마이크로초 실수(예: 1732700000.123456)일 수
//! 있으므로, 시간 클레임은 라이브러리 기본 검증(u64 전용)을 끄고 실수 허용 비교로 직접 검증한다.
//! 즉 PHP 발급 토큰 ↔ Rust 발급 토큰을 양쪽 서버가 서로 검증할 수 있다(무중단 교체 전제).

use std::collections::HashSet;

use anyhow::{bail, Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation};
use rsa::pkcs1::DecodeRsaPublicKey;
use rsa::pkcs8::DecodePublicKey;
use rsa::traits::PublicKeyParts;
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::config::{Config, JwtAlgorithm};
use crate::domain::UserRole;
use crate::error::AppError;

pub struct JwtService {
    algorithm: Algorithm,
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    validation: Validation,
    kid: Option<String>,
    /// 미리 직렬화한 JWKS 응답 본문
    pub jwks_body: String,
    access_ttl: i64,
}

#[derive(Serialize)]
struct AccessClaims<'a> {
    iat: i64,
    nbf: i64,
    exp: i64,
    sub: String,
    aff: &'a str,
    role: i64,
}

/// 검증 결과 — 요청 컨텍스트에 실리는 인증 정보
#[derive(Debug, Clone, Copy)]
pub struct AuthContext {
    pub user_id: i64,
    pub role: UserRole,
}

impl JwtService {
    pub fn new(config: &Config) -> Result<Self> {
        let (algorithm, encoding_key, decoding_key, jwk) = match config.jwt_algo {
            JwtAlgorithm::Hs256 => (
                Algorithm::HS256,
                EncodingKey::from_secret(config.jwt_secret.as_bytes()),
                DecodingKey::from_secret(config.jwt_secret.as_bytes()),
                None,
            ),
            JwtAlgorithm::Rs256 => {
                if !config.jwt_private_key_passphrase.is_empty() {
                    bail!("Rust 구현은 암호화된 개인키(JWT_PRIVATE_KEY_PASSPHRASE)를 지원하지 않습니다 — 복호화된 PEM 을 배치하세요.");
                }
                let private_pem = std::fs::read(&config.jwt_private_key_path)
                    .with_context(|| format!("JWT 개인키 파일을 읽을 수 없습니다: {}", config.jwt_private_key_path.display()))?;
                let public_pem = std::fs::read_to_string(&config.jwt_public_key_path)
                    .with_context(|| format!("공개키 파일을 읽을 수 없습니다: {}", config.jwt_public_key_path.display()))?;
                (
                    Algorithm::RS256,
                    EncodingKey::from_rsa_pem(&private_pem).context("JWT 개인키 PEM 파싱에 실패했습니다.")?,
                    DecodingKey::from_rsa_pem(public_pem.as_bytes()).context("공개키 PEM 파싱에 실패했습니다.")?,
                    Some(rsa_public_jwk(&public_pem, config.jwt_algo)?),
                )
            }
        };

        // 서명·알고리즘만 라이브러리가 검증하고, 시간 클레임은 verify() 에서 PHP 와 같은 순서로 검사한다.
        let mut validation = Validation::new(algorithm);
        validation.validate_exp = false;
        validation.validate_nbf = false;
        validation.validate_aud = false;
        validation.required_spec_claims = HashSet::new();

        let kid = jwk.as_ref().and_then(|j| j["kid"].as_str().map(str::to_string));
        let keys = jwk.map(|j| vec![j]).unwrap_or_default();
        let jwks_body = serde_json::to_string(&json!({ "keys": keys }))?;

        Ok(Self {
            algorithm,
            encoding_key,
            decoding_key,
            validation,
            kid,
            jwks_body,
            access_ttl: config.jwt_access_ttl,
        })
    }

    pub fn access_ttl(&self) -> i64 {
        self.access_ttl
    }

    /// Access 토큰 발급 — iat·nbf=now, exp=now+TTL, sub=userId, aff, role
    pub fn issue_access_token(&self, user_id: i64, affiliation: &str, role: UserRole) -> Result<String, AppError> {
        let now = chrono::Utc::now().timestamp();
        let claims = AccessClaims {
            iat: now,
            nbf: now,
            exp: now + self.access_ttl,
            sub: user_id.to_string(),
            aff: affiliation,
            role: role.value(),
        };
        let mut header = Header::new(self.algorithm);
        header.kid = self.kid.clone();
        jsonwebtoken::encode(&header, &claims, &self.encoding_key).map_err(|e| AppError::Internal(e.to_string()))
    }

    /// `Authorization` 헤더 값을 검증한다 (PHP JwtAuthMiddleware::authenticate 와 동일 순서·메시지)
    pub fn authenticate(&self, header_line: &str) -> Result<AuthContext, AppError> {
        let Some(jwt) = header_line.strip_prefix("Bearer ") else {
            return Err(AppError::Unauthorized("인증 토큰이 필요합니다.".into()));
        };
        if jwt.is_empty() {
            return Err(AppError::InvalidToken("토큰이 비어 있습니다.".into()));
        }
        // 파싱 + 서명(SignedWith) — 실패는 모두 INVALID_TOKEN
        let data = jsonwebtoken::decode::<Value>(jwt, &self.decoding_key, &self.validation)
            .map_err(|_| AppError::invalid_token())?;
        let claims = data.claims;
        let Value::Object(claims) = claims else { return Err(AppError::invalid_token()) };

        // 시간 클레임은 lcobucci 파서처럼 숫자(정수/실수/숫자 문자열)여야 한다 — 아니면 파싱 실패
        let time_claim = |name: &str| -> Result<Option<f64>, AppError> {
            match claims.get(name) {
                None => Ok(None),
                Some(v) => crate::php::numeric_f64(v).map(Some).ok_or_else(AppError::invalid_token),
            }
        };
        let iat = time_claim("iat")?;
        let nbf = time_claim("nbf")?;
        let exp = time_claim("exp")?;

        let now = now_f64();
        // 만료는 TOKEN_EXPIRED 로 구분 (lcobucci isExpired: now >= exp)
        if exp.is_some_and(|e| now >= e) {
            return Err(AppError::token_expired());
        }
        // StrictValidAt — iat·nbf·exp 모두 필수, 미래 발급·미도래 거부
        match (iat, nbf, exp) {
            (Some(iat), Some(nbf), Some(_)) if iat <= now && nbf <= now => {}
            _ => return Err(AppError::invalid_token()),
        }

        let sub = claims.get("sub").filter(|v| crate::php::is_numeric(v));
        let Some(user_id) = sub.and_then(crate::php::numeric_to_int) else {
            return Err(AppError::InvalidToken("토큰에 유효한 사용자 식별자가 없습니다.".into()));
        };
        let role = claims
            .get("role")
            .filter(|v| crate::php::is_numeric(v))
            .and_then(crate::php::numeric_to_int)
            .and_then(UserRole::try_from_int)
            .ok_or_else(|| AppError::InvalidToken("토큰에 유효한 회원구분이 없습니다.".into()))?;
        Ok(AuthContext { user_id, role })
    }
}

fn now_f64() -> f64 {
    let now = chrono::Utc::now();
    now.timestamp() as f64 + f64::from(now.timestamp_subsec_micros()) / 1_000_000.0
}

/// 공개키 PEM → RFC 7517 JWK (kid = RFC 7638 썸프린트)
fn rsa_public_jwk(pem: &str, algo: JwtAlgorithm) -> Result<Value> {
    let key = rsa::RsaPublicKey::from_public_key_pem(pem)
        .or_else(|_| rsa::RsaPublicKey::from_pkcs1_pem(pem))
        .context("공개키 PEM 파싱에 실패했습니다.")?;
    let n = URL_SAFE_NO_PAD.encode(key.n().to_bytes_be());
    let e = URL_SAFE_NO_PAD.encode(key.e().to_bytes_be());
    let canonical = format!("{{\"e\":\"{e}\",\"kty\":\"RSA\",\"n\":\"{n}\"}}");
    let kid = URL_SAFE_NO_PAD.encode(Sha256::digest(canonical.as_bytes()));
    Ok(json!({
        "kty": "RSA",
        "use": "sig",
        "alg": algo.as_str(),
        "kid": kid,
        "n": n,
        "e": e,
    }))
}

/// SHA-256 hex — Refresh·이메일 인증 토큰 저장용 해시
pub fn sha256_hex(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

/// `bin2hex(random_bytes(32))`
pub fn random_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}
