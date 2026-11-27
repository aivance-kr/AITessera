# AITessera — Rust (Axum + SQLx) 포팅

PHP 구현(`src/`)과 **같은 요청 파라미터·응답 본문·상태코드·에러 코드/메시지**를 갖는 Rust 서버다.
같은 MySQL 스키마·Redis 큐·`.env`·JWT 키를 그대로 사용하므로, PHP 서버 옆에 띄워 성능을 비교한 뒤
웹서버(Apache) 프록시 대상만 바꿔 교체할 수 있다.

| 구분 | PHP | Rust |
|------|-----|------|
| HTTP | PSR-15 Relay + FastRoute | Axum 0.8 (+ tower-http) |
| DB | PDO (prepared) | SQLx 0.8 MySQL (prepared, 커넥션 풀) |
| JWT | lcobucci/jwt | jsonwebtoken (알고리즘 고정, HS256/RS256) |
| 비밀번호 | `password_hash(ARGON2ID)` | argon2 (m=65536,t=4,p=1 — PHC 문자열 호환), bcrypt 검증 |
| 레이트 리밋 | symfony sliding_window | 같은 알고리즘을 Redis Lua 로 원자 실행 |
| 큐 | Redis LPUSH + `json_encode` | 같은 채널·같은 JSON 인코딩 (PHP 워커가 그대로 소비) |

## 구조

```
rust/
├── Cargo.toml
├── static/openapi.json     # PHP 어트리뷰트에서 생성한 스펙 스냅샷 (컴파일 시 내장)
├── scripts/
│   ├── parity_check.py     # PHP ↔ Rust 응답 동등성 검사 (81개 시나리오)
│   ├── bench.sh            # 부하 비교 (oha 또는 ab)
│   └── dump_openapi.php    # openapi.json 스냅샷 재생성
└── src/
    ├── main.rs             # 부팅: .env → 설정 → 풀/Redis → 라우터
    ├── config.rs           # Config::fromEnv 와 같은 키·기본값·fail-fast
    ├── http.rs             # 라우트·미들웨어(Cors/RateLimit/JwtAuth/RoleGuard)·컨트롤러
    ├── service.rs          # AuthService / UserService / UserAdminService / LogService
    ├── repo.rs             # Repository (SQL 은 PHP 와 동일, 바인딩만 사용)
    ├── domain.rs           # enum·요청 DTO·검증(Respect\Validation 메시지 재현)·프로필 스키마
    ├── jwt.rs              # 발급·검증·JWKS
    ├── infra.rs            # Redis 큐·레이트 리밋·Argon2
    ├── php.rs              # PHP 의미론 재현(is_numeric, empty, FILTER_VALIDATE_EMAIL …)
    ├── error.rs / response.rs / state.rs
```

미들웨어 순서는 PHP 파이프라인과 같다 —
`ErrorHandler → TrailingSlash → Cors → RateLimit → JwtAuth → RoleGuard → RouteDispatch`.
그래서 토큰 없이 없는 경로를 호출하면 404 가 아니라 401, 예외로 끝난 응답에는 CORS 헤더가 없는 점까지 같다.

## 실행

```bash
cd rust
cargo build --release
./target/release/aitessera          # 기본 127.0.0.1:8306
```

`.env` 는 현재 디렉터리부터 상위로 찾는다(저장소 루트 `.env` 를 그대로 사용). 프로세스 환경변수가 `.env` 보다 우선한다.

Rust 전용 추가 환경변수:

| 키 | 기본값 | 설명 |
|----|--------|------|
| `RUST_BIND_ADDR` | `127.0.0.1:8306` | 리슨 주소 |
| `DB_POOL_MAX` | `32` | SQLx 커넥션 풀 크기 |
| `APP_TIMEZONE` | `UTC` | **PHP `date.timezone` 과 같아야 한다** — DATETIME 저장/표시 문자열이 이 값 기준 |
| `RUST_LOG` | `info,sqlx=warn` | 로그 레벨 |

## PHP 와 응답 동등성 검사

두 서버를 **같은 테스트 DB** 에 붙여 띄운 뒤 실행한다. 운영/개발 DB 를 쓰지 말 것.

```bash
# 테스트 DB 마이그레이션 — PHP 는 variables_order=GPCS 라 프로세스 환경변수가 $_ENV 에 안 들어가므로 EGPCS 로 띄운다
export DB_NAME=aitessera_rsbench RATE_LIMIT_API=1000000 RATE_LIMIT_AUTH=1000000
export JWT_PRIVATE_KEY_PATH=$PWD/var/keys/jwt_private.pem JWT_PUBLIC_KEY_PATH=$PWD/var/keys/jwt_public.pem
php -d variables_order=EGPCS vendor/bin/phinx migrate -c phinx.php
php -d variables_order=EGPCS vendor/bin/phinx seed:run -c phinx.php

PHP_CLI_SERVER_WORKERS=8 php -d variables_order=EGPCS -S 127.0.0.1:18305 -t public &
RUST_BIND_ADDR=127.0.0.1:18306 rust/target/release/aitessera &

python3 rust/scripts/parity_check.py --php http://127.0.0.1:18305 --rust http://127.0.0.1:18306
```

> `php -S` 에서는 상대 경로 키(`var/keys/...`)가 `public/` 기준으로 해석돼 부팅이 실패하므로 절대 경로를 넘긴다.

최근 결과: **81 통과 / 0 불일치 / 0 알려진 차이** — 상호 운용(PHP 발급 토큰을 Rust 가, Rust 발급 토큰을 PHP 가 검증) 포함.

## 부하 비교

```bash
PHP_URL=http://127.0.0.1:18305 RUST_URL=http://127.0.0.1:18306 REQUESTS=2000 CONCURRENCY=10 rust/scripts/bench.sh
```

로컬 측정(Apple Silicon 10코어, MySQL 9.7·Redis 로컬, `ab -k`, 동시성 10, PHP 는 `php -S` 워커 8 + OPcache):

| 시나리오 | PHP rps | PHP p99 | Rust rps | Rust p99 | 배수 |
|----------|--------:|--------:|---------:|---------:|-----:|
| GET /health (DB 없음) | 2,989 | 6.5ms | 41,224 | 0.4ms | 13.8x |
| GET /api/v1/me (JWT+1쿼리) | 1,936 | 10.4ms | 10,757 | 1.3ms | 5.6x |
| GET /api/v1/users (목록 2쿼리) | 1,720 | 11.2ms | 5,246 | 2.5ms | 3.0x |
| POST /api/v1/logs (Redis 큐) | 2,385 | 8.3ms | 14,607 | 1.1ms | 6.1x |
| POST /api/v1/tokens (Argon2id) | 30 | 620ms | 49 | 335ms | 1.6x |

- 로그인은 Argon2id(64MiB·4회)가 지배적이라 언어 차이가 작다. 해시 파라미터는 PHP 와 호환을 위해 그대로 두었다.
- `php -S` 는 연결마다 소켓을 닫아(keep-alive 없음) 불리하다. **교체 판단용 수치는 운영과 같은 Apache mod_php / php-fpm 으로 다시 측정할 것.**
- `php -S` 상대로 대량 요청을 연속 실행하면 macOS 임시 포트가 TIME_WAIT 로 고갈될 수 있다(`apr_socket_connect(): Can't assign requested address`). 잠시 기다렸다 다시 실행한다.
- 부하 테스트는 큐 메시지를 대량으로 적재하므로 개발용 Redis 대신 별도 인스턴스(`REDIS_PORT=16379` 등)를 쓰는 것을 권장한다.

## 교체 절차 (예: Apache)

1. 운영 서버에 바이너리 배치 후 systemd 등으로 `RUST_BIND_ADDR=127.0.0.1:8306` 기동 (같은 `.env`).
2. `APP_TIMEZONE` 을 운영 PHP `date.timezone` 과 맞춘다.
3. Apache 가상호스트에서 `ProxyPass / http://127.0.0.1:8306/` + `ProxyPreserveHost On` 으로 전환. 롤백은 프록시 설정만 되돌리면 된다.
4. 큐 워커(`mail:work`, `log:work`, `security:scan`, 리포트)·마이그레이션(`phinx`)·`seed:admin` 은 계속 PHP CLI 를 사용한다 — 이 포팅은 HTTP API 만 대상이다.
5. 프록시 뒤에서는 클라이언트 IP 가 프록시 주소가 된다(PHP 도 REMOTE_ADDR 만 신뢰하므로 동일 정책). 레이트 리밋 키가 프록시 IP 로 모이지 않게 하려면 별도 결정이 필요하다.

## 알려진 차이·제약

| 항목 | 내용 |
|------|------|
| 레이트 리밋 상태 | 알고리즘은 같지만 Redis 키(`ratelimit:rust:*`)가 별도라 전환 시점에 카운터가 이어지지 않는다(최대 1분 영향) |
| multipart 본문 | PHP 는 `multipart/form-data` POST 를 `$_POST` 로 파싱하지만 Rust 는 빈 입력으로 처리 (현재 클라이언트는 JSON 만 사용) |
| `logged_at` 검증 | Respect `DateTime()`=`strtotime()` 의 자연어 표현 전부는 재현하지 않고 ISO-8601·`Y-m-d H:i:s`·날짜·`@ts`·now/today 등 실사용 형식만 허용 |
| RS256 암호화 개인키 | `JWT_PRIVATE_KEY_PASSPHRASE` 미지원 — 복호화된 PEM 필요 (설정 시 부팅 실패로 알림) |
| 상대 키 경로 | Rust 는 `.env` 위치 기준으로 해석, PHP 는 프로세스 CWD 기준 |
| JWT 시간 클레임 | PHP 는 마이크로초 실수(`1732700000.123456`), Rust 는 정수로 발급. 양쪽 모두 상대 형식을 검증 가능 |
| OpenAPI 스펙 | 컴파일 시 내장한 스냅샷. PHP 어트리뷰트를 바꾸면 `php rust/scripts/dump_openapi.php > rust/static/openapi.json` 후 재빌드 |
| `/api/docs` | PHP 와 같은 HTML (unpkg RapiDoc 을 SRI 없이 로드하는 점도 동일 — 필요 시 양쪽 함께 수정) |

## 테스트

```bash
cargo test     # PHP 의미론 단위 테스트 (FILTER_VALIDATE_EMAIL 실측 대조 포함)
cargo clippy --all-targets
```
