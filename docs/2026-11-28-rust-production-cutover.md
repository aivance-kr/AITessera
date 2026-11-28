# 운영 서버 Rust 전환 런북 (PHP → Rust)

> 작성: 2026-11-28 · 대상: `tessera.aivance.kr` (운영 서버 1대, `ssh prod`)
>
> Rust 포팅(`rust/`, PR #99)은 완료됐고 응답 동등성 검사(87개 시나리오)와 운영 서버 성능 비교
> ([2026-11-27-php-rust-performance.md](2026-11-27-php-rust-performance.md))도 끝났다.
> 이 문서는 **운영 트래픽을 PHP에서 Rust로 넘기는 절차**만 다룬다. 포팅 구조·차이점은
> [`rust/README.md`](../rust/README.md)를 본다.

## 0. 범위

| 바뀌는 것 | 그대로인 것 |
|-----------|-------------|
| `tessera.aivance.kr` HTTPS 요청을 처리하는 프로세스 (PHP-FPM → Rust) | MySQL 스키마, Redis 큐·키, `.env`, JWT 키 |
| Apache 가상호스트에 리버스 프록시 추가 | 큐 워커(`mail:work`, `log:work`, `security:scan`, 리포트), `migrate`, `operator:create` — 계속 PHP CLI |
| systemd 서비스 `aitessera-rust` 추가 | 배포 워크플로(`deploy.yml`) — 이미 Rust 바이너리를 빌드·업로드·교체한다 |

전환 중에도 PHP 코드는 서버에 그대로 남는다. 롤백은 Apache 설정 몇 줄을 되돌리는 것으로 끝난다.

## 1. 현재 운영 서버 상태 (2026-11-28 점검)

읽기 전용 계정으로 확인한 값이다. 전환 직전에 §3의 점검 명령으로 다시 확인한다.

| 항목 | 상태 |
|------|------|
| OS / glibc | Ubuntu 24.04.5 / glibc 2.39, 6코어, 메모리 16GB (가용 약 10GB) |
| 저장소 경로 | `/home/aitessera/AITessera` |
| Rust 바이너리 | `rust/target/release/aitessera` 배포돼 있음 (배포 워크플로가 업로드) |
| Rust 프로세스 | **떠 있지 않음.** 8306 포트 리슨 없음 |
| systemd 유닛 | **없음** (`aitessera-rust.service` 미등록) — 배포의 재시작 단계가 건너뛰는 중 |
| Apache | `000-default-le-ssl.conf` 안의 `tessera.aivance.kr` `*:443` 블록. 프록시 설정 없음 (`DocumentRoot` + `.htaccess` → `index.php`) |
| Apache 모듈 | `proxy`, `proxy_http`, `headers`, `rewrite` 켜져 있음. `remoteip` 꺼져 있음 |
| PHP 시간대 | CLI `date.timezone = UTC`, FPM php.ini 미설정(기본 UTC) |
| JWT 키 | `var/keys/jwt_private.pem` (`aitessera:www-data`, 660), `jwt_public.pem` |
| 정적 파일 | `public/`의 `favicon.ico`, `favicon.svg`, `apple-touch-icon.png`, `icon-512.png`, `logo.png`, `logo.svg` |

## 2. 선행 작업 (전환 전에 끝나야 한다)

### 2.1 [필수·코드] 프록시 뒤 클라이언트 IP — 구현됨, 운영 배포 필요

Rust는 소켓 주소(`ConnectInfo`)만 클라이언트 IP로 쓴다 (`rust/src/http.rs`의 `client_ip`).
Apache 뒤에 두면 **모든 요청의 IP가 `127.0.0.1`** 이 된다. 그러면:

- 레이트 리밋 버킷이 전 사용자 공용이 된다. 로그인·회원가입·인증 경로는 **서비스 전체가 분당 10회**로 막힌다.
- `login_events`의 IP가 전부 `127.0.0.1`로 기록돼 로그인 이상 탐지가 무력화된다.

지금의 PHP는 Apache 안에서 실제 접속 IP(`REMOTE_ADDR`)를 받으므로 이 문제가 없다. Rust에는 다음 규칙이 들어가 있다 (`resolve_client_ip`).

- 직접 연결한 쪽(peer)이 루프백(`127.0.0.1`/`::1`)일 때만 `X-Forwarded-For`를 본다.
- 그 헤더의 **마지막 값**만 쓴다. Apache `mod_proxy`는 실제 접속 IP를 헤더 끝에 덧붙이므로 마지막 값은 위조할 수 없고, 앞쪽 값은 클라이언트가 넣은 것일 수 있다.
- peer가 루프백이 아니면 지금처럼 소켓 주소를 쓴다.
- 마지막 값이 IP로 해석되지 않으면(포트 포함, `unknown` 등) peer 주소를 쓴다.
- 레이트 리밋(`client_ip()`)과 로그인 이벤트(`token_issue`의 `LoginContext.ip`)가 같은 함수를 쓴다.

전환 전에 이 변경이 `dev` → `main` 배포로 운영 바이너리에 들어가 있어야 한다. 실제 IP 기록은 §4.4에서 확인한다.

### 2.2 [필수] systemd 유닛 등록과 sudoers

유닛 원본은 저장소의 [`deploy/aitessera-rust.service`](../deploy/aitessera-rust.service)다. 서버에는 링크만 건다.
sudo 권한이 있는 운영자가 실행한다 (Claude의 `claude-ro` 계정은 진단 전용이라 실행하지 않는다).

```bash
sudo systemctl link /home/aitessera/AITessera/deploy/aitessera-rust.service
```

```bash
sudo systemctl daemon-reload
```

배포 워크플로가 바이너리 교체 뒤 `sudo -n systemctl restart aitessera-rust`를 실행하므로, 배포 계정에 그 권한을 준다.
기존 `/etc/sudoers.d/aitessera-deploy`에 한 줄을 추가한다 (`<DEPLOY_USER>`는 기존 줄과 같은 계정).

```bash
sudo visudo -f /etc/sudoers.d/aitessera-deploy
```

추가할 줄:

```
<DEPLOY_USER> ALL=(ALL) NOPASSWD: /usr/bin/systemctl restart aitessera-rust
```

> 유닛 파일이 바뀐 배포 뒤에는 `sudo systemctl daemon-reload`를 다시 해야 한다. 배포 워크플로는 이를 하지 않는다.

### 2.3 [권장] 운영 `.env` 정리

점검 중 발견한 것이다. 전환과 무관하게 정리한다. 값은 이 문서에 적지 않는다.

| 키 | 문제 | 조치 |
|----|------|------|
| `RATE_LIMIT_APIZZ` | 오타. 실제 키 `RATE_LIMIT_API`가 없어서 기본값 120이 적용 중 | `RATE_LIMIT_API`로 고치거나 줄을 지운다 (의도한 값이 120이면 삭제) |
| `ADMIN_EMAIL`, `ADMIN_PASSWORD`, `ADMIN_AFFILIATION` | 존재하지 않던 `seed:admin`용 키. 코드가 읽지 않는데 평문 비밀번호가 남아 있다 | 삭제. 그 계정 비밀번호를 다른 곳에서 쓰고 있었다면 변경 |

## 3. 전환 전 점검 (D-0, 운영자)

```bash
cd /home/aitessera/AITessera && sudo -u aitessera git log --oneline -1
```

`origin/main`의 최신 커밋과 같아야 한다 (§2.1 변경 포함).

```bash
ldd /home/aitessera/AITessera/rust/target/release/aitessera | grep 'not found' || echo OK
```

```bash
php -r 'echo date_default_timezone_get(), PHP_EOL;'
```

`UTC`여야 한다. 다르면 유닛의 `APP_TIMEZONE`을 같은 값으로 바꾼다.

```bash
sudo crontab -u aitessera -l
```

큐 워커(`mail:work`, `log:work` 등)가 어떻게 돌고 있는지 확인만 한다. 전환 후에도 그대로 둔다.

## 4. 전환 절차

### 4.1 Rust 서비스 기동 (트래픽 없음)

```bash
sudo systemctl enable --now aitessera-rust
```

```bash
systemctl status aitessera-rust --no-pager
```

```bash
curl -s http://127.0.0.1:8306/health
```

부팅이 실패하면 `journalctl -u aitessera-rust -n 50 --no-pager`를 확인한다. 흔한 원인은 `.env`를 못 찾음(`WorkingDirectory`), 키 파일 권한, `APP_TIMEZONE` 값 오류다.
이 단계에서는 외부 트래픽이 PHP로만 가므로 영향이 없다.

### 4.2 루프백 스모크 테스트

```bash
curl -s http://127.0.0.1:8306/.well-known/jwks.json
```

PHP와 같은 `kid`인지 비교한다:

```bash
curl -s --resolve tessera.aivance.kr:443:127.0.0.1 https://tessera.aivance.kr/.well-known/jwks.json
```

토큰 교차 검증 — PHP에서 받은 Access 토큰으로 Rust의 `/api/v1/me`를 호출해 200이 나오는지 본다. 운영 계정이 필요하므로
운영자가 직접 수행하고, 토큰·비밀번호를 기록이나 셸 히스토리에 남기지 않는다.

### 4.3 Apache 설정 변경

1) 백업:

```bash
sudo cp /etc/apache2/sites-available/000-default-le-ssl.conf /etc/apache2/sites-available/000-default-le-ssl.conf.bak-$(date +%Y%m%d%H%M)
```

> `sites-enabled/000-default-le-ssl.conf`가 `sites-available`의 링크인지 먼저 `ls -l`로 확인하고, 실제 파일을 고친다.
> 이 파일은 다른 서비스 vhost와 certbot 설정이 함께 들어 있다. **`tessera.aivance.kr` `*:443` 블록만** 고친다.

2) `tessera.aivance.kr` `*:443` 블록의 `</VirtualHost>` 바로 위에 추가:

```apache
    # Rust(aitessera-rust) 로 API 전달 — 롤백은 아래 4줄 주석 처리
    ProxyPreserveHost On
    ProxyPassMatch "^/(favicon\.(ico|svg)|apple-touch-icon\.png|icon-512\.png|logo\.(png|svg))$" "!"
    ProxyPass        / http://127.0.0.1:8306/ retry=0 timeout=30
    ProxyPassReverse / http://127.0.0.1:8306/
```

- `DocumentRoot`, `<Directory>`, `.htaccess`는 지우지 않는다. 정적 파일(`ProxyPassMatch ... "!"`)과 롤백에 필요하다.
- `ProxyPass`가 `.htaccess`의 `index.php` 재작성보다 먼저 처리되므로 PHP로는 요청이 가지 않는다.
- 80 포트 블록은 HTTPS 리다이렉트만 하므로 고치지 않는다.

3) 문법 확인 후 반영:

```bash
sudo apache2ctl configtest
```

`Syntax OK`일 때만:

```bash
sudo systemctl reload apache2
```

### 4.4 전환 확인 (직후 10분)

어느 구현이 응답하는지 확인 — multipart 로그인은 PHP면 `INVALID_CREDENTIALS`, Rust면 `VALIDATION_ERROR`:

```bash
curl -s -F email=probe@example.com -F password=x https://tessera.aivance.kr/api/v1/tokens
```

`VALIDATION_ERROR`가 나와야 한다.

```bash
curl -s https://tessera.aivance.kr/health
```

```bash
curl -sI https://tessera.aivance.kr/favicon.svg | head -1
```

정적 파일은 200이어야 한다 (Apache가 직접 서빙).

클라이언트 IP가 실제 IP로 들어오는지 (§2.1 확인):

```bash
journalctl -u aitessera-rust --since '-10 min' --no-pager | tail -50
```

```bash
sudo tail -f /var/log/apache2/tessera_access.log
```

로그인 이후 `login_events`에 쌓인 최근 행의 IP가 `127.0.0.1`이 아닌지 DB에서 확인한다.

### 4.5 관찰 기간 (1주)

| 지표 | 확인 방법 | 롤백 기준 |
|------|-----------|-----------|
| 5xx 비율 | `tessera_access.log` 상태 코드 집계 | 전환 전 PHP 기준선보다 높음 |
| 429 비율 | 같은 로그 | 전환 전보다 눈에 띄게 늘어남 (IP 처리 이상 신호) |
| 401 비율 | 같은 로그 | 급증 (토큰 검증 차이) |
| 프로세스 재시작 | `systemctl show aitessera-rust -p NRestarts` | 0이 아님 |
| 메모리 | `systemctl status aitessera-rust` | 로그인 몰림 시 일시적으로 수백 MB까지는 정상 (Argon2 64MiB × 동시 해시 수, 코어 수로 제한) |
| 큐 소비 | 메일 발송·로그 적재가 계속 되는지 | 멈춤 (Rust가 넣은 메시지를 PHP 워커가 처리 못함) |

## 5. 롤백

Rust를 끄기 전에 트래픽부터 PHP로 되돌린다.

1) §4.3에서 추가한 4줄을 주석 처리하거나 백업 파일로 되돌린다.

2) 반영:

```bash
sudo apache2ctl configtest
```

```bash
sudo systemctl reload apache2
```

3) multipart 로그인 확인(§4.4)이 `INVALID_CREDENTIALS`로 돌아오면 롤백 완료. Rust 서비스는 원인 조사를 위해 켜 둬도 된다.

롤백해도 데이터 정리는 필요 없다. 두 구현이 같은 DB·Redis·JWT 키를 쓰므로 Rust가 발급한 토큰과 세션은 PHP에서 그대로 유효하다.
레이트 리밋 카운터만 Redis 키가 달라(`ratelimit:rust:*`) 전환·롤백 순간 초기화된다 (최대 1분 영향).

## 6. 전환 후 배포

- `dev → main` 머지 시 `deploy.yml`이 바이너리를 빌드·업로드하고 `sha256`·`ldd` 확인 후 교체, `aitessera-rust`를 재시작한다.
- 재시작 동안(수백 ms) Apache는 `503`을 돌려준다 (`retry=0`이라 다음 요청부터 바로 재연결). 무중단이 필요해지면 소켓 활성화나 2인스턴스 교대를 따로 설계한다.
- **PHP와 Rust는 함께 바꾼다.** API 동작을 바꾸는 변경은 양쪽에 같이 넣고 `rust/scripts/parity_check.py`가 0건 불일치여야 한다. PHP는 롤백 경로이고 CLI 워커가 여전히 PHP다.
- PHP 어트리뷰트(OpenAPI)를 바꾸면 `rust/static/openapi.json`을 다시 만든다 (`rust/README.md` 참조).

## 7. 알려진 차이 (전환 영향)

`rust/README.md` "알려진 차이·제약" 중 운영에 영향이 있는 것만 추렸다.

| 항목 | 영향 | 판단 |
|------|------|------|
| `multipart/form-data` POST | Rust는 빈 입력(`VALIDATION_ERROR`)으로 처리 | 현재 클라이언트는 JSON만 사용. 전환 후 422 급증이 보이면 이 원인부터 의심 |
| `logged_at` 자연어 형식 | `strtotime` 자연어 일부 미지원 | 실사용 형식(ISO-8601 등)은 지원 |
| `JWT_PRIVATE_KEY_PASSPHRASE` | 미지원 (설정 시 부팅 실패) | 운영 `.env`에 이 키 없음 — 해당 없음 |
| JWT 시간 클레임 | PHP는 소수, Rust는 정수 | 양쪽이 서로의 형식을 검증 가능 — 영향 없음 |

## 8. 체크리스트

- [ ] §2.1 프록시 IP 처리 코드(`resolve_client_ip`) 운영 배포 완료
- [ ] §2.2 유닛 링크, sudoers 한 줄 추가
- [ ] §2.3 `.env` 오타·잔여 키 정리
- [ ] §3 점검 통과
- [ ] §4.1 서비스 기동, `/health` 응답
- [ ] §4.2 JWKS `kid` 일치, 교차 토큰 검증 200
- [ ] §4.3 Apache 백업, 설정 추가, `configtest` 통과, reload
- [ ] §4.4 Rust 응답 확인, 정적 파일 200, 실제 클라이언트 IP 기록
- [ ] §4.5 1주 관찰, 롤백 기준 미달
- [ ] `CLAUDE.md` 배포 절차(Apache/OPcache 중심 설명)를 Rust 기준으로 갱신
