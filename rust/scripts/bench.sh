#!/usr/bin/env bash
# PHP ↔ Rust 부하 비교 — 같은 시나리오를 두 서버에 보내 RPS·지연(p50/p99)을 표로 출력한다.
#
# 사용법:
#   PHP_URL=http://127.0.0.1:18305 RUST_URL=http://127.0.0.1:18306 rust/scripts/bench.sh
#
# 환경변수 (기본값):
#   PHP_URL / RUST_URL       비교 대상 서버
#   REQUESTS=5000            시나리오별 요청 수 (로그인은 REQUESTS/10 — Argon2id 가 의도적으로 무겁다)
#   CONCURRENCY=50           동시 연결 수
#   ADMIN_EMAIL / ADMIN_PASSWORD / ADMIN_AFFILIATION   운영자 계정 (TestUserSeeder 기본값)
#   SCENARIOS="health me users logs login"             실행할 시나리오
#
# 도구: oha 가 있으면 oha(HTTP/1.1 keep-alive), 없으면 ab(-k) 를 쓴다.
# 주의: 레이트 리밋에 걸리지 않도록 두 서버 모두 RATE_LIMIT_API/RATE_LIMIT_AUTH 를 충분히 크게 띄울 것.
#       PHP 는 운영과 같은 Apache mod_php / php-fpm 으로 띄워야 공정하다 (php -S 는 참고용).
set -euo pipefail

PHP_URL="${PHP_URL:-http://127.0.0.1:18305}"
RUST_URL="${RUST_URL:-http://127.0.0.1:18306}"
REQUESTS="${REQUESTS:-5000}"
CONCURRENCY="${CONCURRENCY:-50}"
ADMIN_EMAIL="${ADMIN_EMAIL:-admin@aivance.test}"
ADMIN_PASSWORD="${ADMIN_PASSWORD:-password1234!}"
ADMIN_AFFILIATION="${ADMIN_AFFILIATION:-aivance}"
SCENARIOS="${SCENARIOS:-health me users logs login}"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

LOGIN_BODY="$WORK/login.json"
printf '{"email":"%s","password":"%s","affiliation":"%s"}' "$ADMIN_EMAIL" "$ADMIN_PASSWORD" "$ADMIN_AFFILIATION" > "$LOGIN_BODY"
LOG_BODY="$WORK/log.json"
printf '{"level":"info","message":"bench","context":{"k":"v"},"source":"bench"}' > "$LOG_BODY"

if command -v oha >/dev/null 2>&1; then TOOL=oha
elif command -v ab >/dev/null 2>&1; then TOOL=ab
else echo "oha 또는 ab 가 필요합니다 (brew install oha)" >&2; exit 1
fi

token_for() {
  curl -fsS -X POST "$1/api/v1/tokens" -H 'Content-Type: application/json' --data-binary "@$LOGIN_BODY" \
    | python3 -c 'import json,sys; print(json.load(sys.stdin)["data"]["access_token"])'
}

# run <base> <method> <path> <n> [token] [body-file] → "rps p50ms p99ms fail"
run() {
  local base="$1" method="$2" path="$3" n="$4" token="${5:-}" body="${6:-}"
  local out="$WORK/out.txt"
  if [[ "$TOOL" == oha ]]; then
    local args=(-n "$n" -c "$CONCURRENCY" --no-tui --output-format json -m "$method")
    [[ -n "$token" ]] && args+=(-H "Authorization: Bearer $token")
    [[ -n "$body" ]] && args+=(-T application/json -D "$body")
    oha "${args[@]}" "$base$path" > "$out"
    python3 - "$out" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
s, p = d["summary"], d["latencyPercentiles"]
codes = d.get("statusCodeDistribution", {})
fail = sum(v for k, v in codes.items() if not k.startswith("2"))
print(f'{s["requestsPerSec"]:.0f} {p["p50"]*1000:.1f} {p["p99"]*1000:.1f} {fail}')
PY
  else
    # ab 는 -p(본문 파일)만으로 POST 가 되며, -m 과 함께 쓰면 측정이 깨진다
    local args=(-k -q -n "$n" -c "$CONCURRENCY" -e "$WORK/pct.csv")
    [[ -z "$body" && "$method" != GET ]] && args+=(-m "$method")
    [[ -n "$token" ]] && args+=(-H "Authorization: Bearer $token")
    [[ -n "$body" ]] && args+=(-p "$body" -T application/json)
    ab "${args[@]}" "$base$path" > "$out" 2>&1 || true
    python3 - "$out" "$WORK/pct.csv" <<'PY'
import re, sys, csv
txt = open(sys.argv[1]).read()
rps = re.search(r"Requests per second:\s+([\d.]+)", txt)
# ab 의 "Failed requests" 는 응답 길이 차이(Length)도 포함한다 — PHP JWT 는 iat 가 마이크로초 실수라
# 길이가 매번 달라지므로 제외하고, 연결·수신·예외 오류와 비2xx 만 실패로 센다
detail = re.search(r"\(Connect: (\d+), Receive: (\d+), Length: \d+, Exceptions: (\d+)\)", txt)
non2xx = re.search(r"Non-2xx responses:\s+(\d+)", txt)
pct = {int(float(r[0])): float(r[1]) for r in list(csv.reader(open(sys.argv[2])))[1:]}
fail = (sum(int(g) for g in detail.groups()) if detail else 0) + int(non2xx.group(1) if non2xx else 0)
print(f'{float(rps.group(1)) if rps else 0:.0f} {pct.get(50, 0):.1f} {pct.get(99, 0):.1f} {fail}')
PY
  fi
}

echo "도구=$TOOL  요청=$REQUESTS  동시성=$CONCURRENCY"
echo "PHP=$PHP_URL  RUST=$RUST_URL"
PHP_TOKEN="$(token_for "$PHP_URL")"
RUST_TOKEN="$(token_for "$RUST_URL")"

printf '\n%-28s | %10s %9s %9s %5s | %10s %9s %9s %5s | %7s\n' \
  "시나리오" "PHP rps" "p50ms" "p99ms" "fail" "Rust rps" "p50ms" "p99ms" "fail" "배수"
printf -- '-%.0s' {1..118}; echo

bench() {
  local name="$1" method="$2" path="$3" n="$4" auth="$5" body="${6:-}"
  local pt="" rt=""
  [[ "$auth" == yes ]] && pt="$PHP_TOKEN" && rt="$RUST_TOKEN"
  # 워밍업 (OPcache·커넥션 풀·JIT 경로 예열)
  run "$PHP_URL" "$method" "$path" 200 "$pt" "$body" >/dev/null
  run "$RUST_URL" "$method" "$path" 200 "$rt" "$body" >/dev/null
  read -r p_rps p_50 p_99 p_f <<<"$(run "$PHP_URL" "$method" "$path" "$n" "$pt" "$body")"
  read -r r_rps r_50 r_99 r_f <<<"$(run "$RUST_URL" "$method" "$path" "$n" "$rt" "$body")"
  local ratio
  ratio="$(python3 -c "print(f'{($r_rps/$p_rps) if $p_rps else 0:.1f}x')")"
  printf '%-28s | %10s %9s %9s %5s | %10s %9s %9s %5s | %7s\n' \
    "$name" "$p_rps" "$p_50" "$p_99" "$p_f" "$r_rps" "$r_50" "$r_99" "$r_f" "$ratio"
}

for s in $SCENARIOS; do
  case "$s" in
    health) bench "GET /health (DB 없음)" GET /health "$REQUESTS" no ;;
    me)     bench "GET /api/v1/me (JWT+1쿼리)" GET /api/v1/me "$REQUESTS" yes ;;
    users)  bench "GET /api/v1/users (목록)" GET "/api/v1/users?per_page=20" "$REQUESTS" yes ;;
    logs)   bench "POST /api/v1/logs (Redis)" POST /api/v1/logs "$REQUESTS" no "$LOG_BODY" ;;
    login)  bench "POST /api/v1/tokens (Argon2)" POST /api/v1/tokens "$((REQUESTS / 10))" no "$LOGIN_BODY" ;;
    *) echo "알 수 없는 시나리오: $s" >&2 ;;
  esac
done
