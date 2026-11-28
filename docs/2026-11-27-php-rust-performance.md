# PHP ↔ Rust API 성능 비교 (운영 서버, 2026-11-27)

`rust/` 의 Axum + SQLx 포팅을 운영 서버(`tessera.aivance.kr`)에 배포한 뒤, 현재 서비스 중인
PHP 구현과 같은 서버·같은 DB·같은 Redis 에서 응답 지연과 처리량을 비교했다.
로컬 측정 결과는 [`rust/README.md`](../rust/README.md) 의 "부하 비교" 절에 있다.

## 요약

- DB·Redis 를 거치는 일반 API 에서 Rust 가 PHP 보다 **평균 지연 7.7~23배 빠르다** (PHP 5.5~7.0ms → Rust 0.2~0.9ms).
- 로그인(`POST /api/v1/tokens`)은 Argon2id 해시가 지배적이라 **1.3배** 차이에 그친다.
- 처리량은 `/health` 기준 PHP 가 약 **710 rps 에서 포화**되고, Rust 는 같은 조건에서 **78,000 rps / p99 2.2ms** 를 낸다.
- 모든 측정 요청은 2xx 로 성공했다 (실패·429 0건).

## 측정 환경

| 항목 | 내용 |
|------|------|
| 서버 | 운영 서버 1대, 6코어 / 16GB, 다른 서비스와 공유 (측정 중 load average 3.2~3.7) |
| PHP | Apache 2.4.58 + PHP-FPM 8.5 (`pm = dynamic`, `max_children = 150`), OPcache·JIT(1255) 켜짐, `APP_ENV=production` |
| Rust | 릴리스 바이너리 `aitessera`, `127.0.0.1:8306` 리슨 (Apache 프록시 미연결) |
| DB / Redis | 두 구현이 같은 MySQL 스키마·같은 Redis 사용 |
| 부하 도구 | 서버 안에서 `ab -k` (Apache Bench, keep-alive) |
| 호출 경로 | PHP: `https://127.0.0.1` + `Host: tessera.aivance.kr` (TLS + Apache + FPM) / Rust: `http://127.0.0.1:8306` 직접 |

외부 네트워크 왕복을 빼고 애플리케이션 차이만 보려고 두 서버 모두 **서버 안의 루프백**으로 호출했다.
Apache·TLS 자체 비용을 분리하려고 같은 가상호스트의 정적 파일(`/favicon.svg`)도 함께 측정했다.

### 레이트 리밋 제약

운영 설정은 일반 API 분당 120회, 인증 엔드포인트 분당 10회(IP 기준)다. 운영 보안 설정을 바꾸지 않기로 하고,
DB·인증 시나리오는 **리밋 안에서 동시성 1로 지연만** 측정했다. 처리량은 리밋이 적용되지 않는 `/health` 만 측정했다.
Rust 는 레이트 리밋 Redis 키(`ratelimit:rust:*`)가 별도라 PHP 와 버킷을 공유하지 않는다.

## 결과

### 지연 (동시성 1)

| 시나리오 | 요청 수 | PHP p50 | PHP p90 | PHP 평균 | Rust p50 | Rust p90 | Rust 평균 | 배수(평균) |
|----------|-------:|-------:|-------:|--------:|--------:|--------:|---------:|-----:|
| `GET /health` (DB 없음) | 5,000 | 3.8ms | 4.9ms | 4.1ms | <0.1ms | <0.1ms | 0.04ms | 약 97x |
| `POST /api/v1/logs` (Redis 큐) | 100 | 5.3ms | 6.1ms | 5.5ms | 0.2ms | 0.2ms | 0.24ms | 23x |
| `GET /api/v1/me` (JWT + 쿼리 1개) | 50 | 6.6ms | 9.6ms | 7.0ms | 0.4ms | 0.6ms | 0.43ms | 16x |
| `GET /api/v1/users?per_page=20` (목록, 쿼리 2개) | 50 | 6.6ms | 7.3ms | 6.9ms | 0.8ms | 1.1ms | 0.90ms | 7.7x |
| `POST /api/v1/tokens` (Argon2id 로그인) | 7 | 242ms | 255ms | 235ms | 186ms | 203ms | 185ms | 1.3x |

> 표본이 작은 시나리오(50건·7건)는 p99 를 신뢰할 수 없어 표에서 뺐다.

### 처리량 (`GET /health`, 시나리오당 5,000건)

| 동시성 | PHP rps | PHP p99 | Apache 정적 파일 rps | 정적 p99 | Rust rps | Rust p99 |
|------:|-------:|-------:|-----------------:|-------:|--------:|--------:|
| 1 | 244 | 7.3ms | 4,158 | 2.0ms | 23,701 | 0.1ms |
| 10 | 715 | 23.0ms | 19,137 | 3.3ms | 63,272 | 1.2ms |
| 50 | 708 | 107.1ms | 19,946 | 52.7ms | 78,471 | 2.2ms |

## 해석

- **PHP 의 고정 비용은 요청당 약 4ms 다.** 같은 https 경로의 정적 파일이 0.2ms 이므로 TLS·Apache 비용이 아니라,
  요청마다 앱을 부팅하는 비용(오토로드, DI 컨테이너 생성, 미들웨어 파이프라인 구성)이 대부분이다.
  DB 쿼리가 붙으면 여기에 2~3ms 가 더해진다.
- **PHP 처리량은 이 서버에서 약 710 rps 가 상한이다.** 동시성 10 이후로 rps 는 늘지 않고 지연만 길어진다
  (동시성 50 에서 p50 70ms). 다른 서비스와 공유하는 6코어에서 CPU 가 먼저 포화된 것으로 본다.
- **로그인은 언어 차이가 작다.** Argon2id(64MiB, 4회) 계산이 응답 시간의 대부분이고, PHP 와 해시 호환을 위해
  Rust 도 같은 파라미터를 쓴다. 로컬 측정(1.6x)과 같은 경향이다.
- **Rust 수치는 Apache 를 거치지 않은 값이다.** 운영 전환 시 Apache 리버스 프록시(TLS 종단 포함)를 거치게 되므로
  정적 파일 측정치 정도(요청당 0.3~1ms)가 더해진다. 그래도 DB 를 쓰는 API 에서 수 배 차이는 유지될 것으로 예상하지만,
  전환 판단 전에 프록시 연결 상태로 한 번 더 측정하는 것이 정확하다.

## 한계와 다음 측정

- DB·인증 시나리오는 레이트 리밋 때문에 동시성 1 지연만 측정했다. 동시 부하에서의 처리량 비교가 필요하면
  측정 시간 동안 `RATE_LIMIT_API`/`RATE_LIMIT_AUTH` 를 올리고 PHP-FPM·Rust 를 재시작해야 한다(운영 보안 설정 변경이므로 별도 결정 필요).
- 서버를 다른 서비스와 공유하므로 측정 시점의 부하에 따라 수치가 흔들릴 수 있다.
- Rust 를 Apache `ProxyPass` 뒤에 연결한 뒤 같은 시나리오를 공개 URL 기준으로 다시 측정한다.

## 측정 부작용

- `POST /api/v1/logs` 측정으로 운영 Redis 로그 큐에 `message: "bench"` 메시지가 PHP·Rust 각 101건 적재됐다(로그 워커가 DB 로 저장).
- 로그인 측정으로 운영자 계정의 리프레시 토큰이 PHP 9개, Rust 8개 발급됐다(만료 시 정리).

## 재현 방법

운영 서버에 SSH 로 접속해 서버 안에서 실행한다. 운영자 계정은 서버 `.env` 의 `ADMIN_EMAIL`/`ADMIN_PASSWORD`/`ADMIN_AFFILIATION`
에서 읽어 임시 파일로만 쓰고, 명령줄·출력에 남기지 않는다.

```bash
# 처리량 — /health (레이트 리밋 제외 경로)
ab -k -q -n 5000 -c 50 -H "Host: tessera.aivance.kr" https://127.0.0.1/health
ab -k -q -n 5000 -c 50 http://127.0.0.1:8306/health

# 지연 — 인증 필요 시나리오 (분당 리밋 안에서 동시성 1)
ab -k -q -n 50 -c 1 -H "Host: tessera.aivance.kr" -H "Authorization: Bearer $PHP_TOKEN" https://127.0.0.1/api/v1/me
ab -k -q -n 50 -c 1 -H "Authorization: Bearer $RUST_TOKEN" http://127.0.0.1:8306/api/v1/me
```

- PHP 토큰 발급은 `curl --resolve tessera.aivance.kr:443:127.0.0.1 https://tessera.aivance.kr/api/v1/tokens` 처럼 SNI 를 맞춰 호출한다.
  `https://127.0.0.1` 에 `Host` 헤더만 붙인 `curl` 은 올바른 가상호스트로 가지 않아 JSON 이 아닌 응답을 받는다.
- `ab` 의 `-e` 백분위 CSV 는 요청 수가 적으면 p99 값이 깨지므로 50건 미만 시나리오에서는 p50·p90·평균만 본다.
- 일반 API 리밋(분당 120회)을 넘지 않도록 시나리오 사이에 1분 간격을 두거나 합계를 120건 이하로 맞춘다.
