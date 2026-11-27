#!/usr/bin/env python3
"""PHP ↔ Rust 응답 동등성 검사.

같은 시나리오를 두 서버에 보내 상태코드·Content-Type·CORS 헤더 유무·JSON 본문을 비교한다.
토큰·ID·시각처럼 실행마다 달라지는 값은 정규화한 뒤 비교한다. 또한 PHP 가 발급한 토큰을
Rust 가, Rust 가 발급한 토큰을 PHP 가 검증하는지(상호 운용)도 확인한다.

사용법:
  python3 rust/scripts/parity_check.py --php http://127.0.0.1:18305 --rust http://127.0.0.1:18306 \
      --admin-email admin@aivance.test --admin-password 'password1234!'

두 서버는 같은 DB 를 써야 하며(테스트 DB 권장), 레이트 리밋은 충분히 높여 둔다.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
import time
import urllib.error
import urllib.request

DYNAMIC_KEYS = {"access_token", "refresh_token", "created_at", "last_login_at", "id"}


def call(base: str, method: str, path: str, body=None, token: str | None = None, raw: bytes | None = None,
         content_type: str = "application/json", headers: dict | None = None):
    data = raw if raw is not None else (json.dumps(body).encode() if body is not None else None)
    req = urllib.request.Request(base + path, data=data, method=method)
    if data is not None:
        req.add_header("Content-Type", content_type)
    if token is not None:
        req.add_header("Authorization", token if token.startswith(("Bearer", "Basic")) or token == "" else f"Bearer {token}")
    for k, v in (headers or {}).items():
        req.add_header(k, v)
    try:
        with urllib.request.urlopen(req, timeout=30) as res:
            return res.status, dict(res.headers), res.read()
    except urllib.error.HTTPError as e:
        return e.code, dict(e.headers), e.read()


def normalize(value, tag: str):
    if isinstance(value, dict):
        return {k: ("<dyn>" if k in DYNAMIC_KEYS and value[k] is not None else normalize(v, tag)) for k, v in value.items()}
    if isinstance(value, list):
        return [normalize(v, tag) for v in value]
    if isinstance(value, str):
        # 서버별 이메일(label-php-…/label-rust-…)은 공유 DB 에서 양쪽 목록에 모두 보이므로 둘 다 정규화
        return value.replace("-php-", "-<tag>-").replace("-rust-", "-<tag>-")
    return value


def summarize(status, headers, body: bytes, tag: str):
    lower = {k.lower(): v for k, v in headers.items()}
    try:
        parsed = normalize(json.loads(body), tag) if body else None
    except ValueError:
        parsed = body.decode(errors="replace")[:200]
    return {
        "status": status,
        "content-type": lower.get("content-type", "").lower().replace(" ", ""),
        "cors": "access-control-allow-origin" in lower,
        "retry-after": "retry-after" in lower,
        "body": parsed,
    }


class Runner:
    def __init__(self, php: str, rust: str):
        self.bases = {"php": php, "rust": rust}
        self.passed = 0
        self.failed: list[str] = []
        self.known: list[str] = []

    def compare(self, name: str, fn, known_diff: str | None = None):
        """fn(server_name, base) -> (status, headers, body). known_diff 는 PHP 버그 등 의도된 차이."""
        results = {}
        for server, base in self.bases.items():
            results[server] = summarize(*fn(server, base), tag=server)
        if known_diff and results["php"] != results["rust"]:
            self.known.append(name)
            print(f"  KNOWN {name} — {known_diff}")
            print(f"       php : status={results['php']['status']}  rust: status={results['rust']['status']}")
            return results
        if results["php"] == results["rust"]:
            self.passed += 1
            print(f"  ok   {name}")
        else:
            self.failed.append(name)
            print(f"  FAIL {name}")
            print(f"       php : {json.dumps(results['php'], ensure_ascii=False)[:600]}")
            print(f"       rust: {json.dumps(results['rust'], ensure_ascii=False)[:600]}")
        return results


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--php", default="http://127.0.0.1:18305")
    ap.add_argument("--rust", default="http://127.0.0.1:18306")
    ap.add_argument("--admin-email", default="admin@aivance.test")
    ap.add_argument("--admin-password", default="password1234!")
    ap.add_argument("--admin-affiliation", default="aivance")
    args = ap.parse_args()

    r = Runner(args.php, args.rust)
    stamp = str(int(time.time()))
    admin = {"email": args.admin_email, "password": args.admin_password, "affiliation": args.admin_affiliation}

    def email(server: str, label: str) -> str:
        return f"{label}-{server}-{stamp}@example.com"

    def login(base: str, creds: dict) -> dict:
        status, _, body = call(base, "POST", "/api/v1/tokens", creds)
        assert status == 201, (base, status, body)
        return json.loads(body)["data"]

    tokens = {s: login(b, admin) for s, b in r.bases.items()}
    access = {s: t["access_token"] for s, t in tokens.items()}

    print("[공개·정적]")
    r.compare("GET /health", lambda s, b: call(b, "GET", "/health"))
    r.compare("GET /health/ (끝 슬래시)", lambda s, b: call(b, "GET", "/health/"))
    r.compare("OPTIONS 프리플라이트", lambda s, b: call(b, "OPTIONS", "/api/v1/me"))
    r.compare("GET /.well-known/jwks.json", lambda s, b: call(b, "GET", "/.well-known/jwks.json"))
    r.compare("GET /api/v1/jwks.json", lambda s, b: call(b, "GET", "/api/v1/jwks.json"))

    print("[인증 미들웨어]")
    r.compare("없는 경로 · 토큰 없음 → 401", lambda s, b: call(b, "GET", "/nope"))
    r.compare("없는 경로 · 토큰 → 404", lambda s, b: call(b, "GET", "/nope", token=access[s]))
    r.compare("Basic 인증 헤더", lambda s, b: call(b, "GET", "/api/v1/me", token="Basic abc"))
    r.compare("빈 Bearer", lambda s, b: call(b, "GET", "/api/v1/me", headers={"Authorization": "Bearer "}))
    r.compare("깨진 토큰", lambda s, b: call(b, "GET", "/api/v1/me", token="abc.def.ghi"))
    r.compare("PUT /api/v1/me → 405", lambda s, b: call(b, "PUT", "/api/v1/me", token=access[s]))
    r.compare("GET /api/v1/users/abc → 404", lambda s, b: call(b, "GET", "/api/v1/users/abc", token=access[s]))
    r.compare("GET /api/v1/users/verify → 405", lambda s, b: call(b, "GET", "/api/v1/users/verify", token=access[s]))
    r.compare("HEAD /health → 401(공개 목록은 GET 만)", lambda s, b: call(b, "HEAD", "/health"))

    print("[토큰 상호 운용]")
    other = {"php": "rust", "rust": "php"}
    r.compare("교차 토큰으로 GET /me", lambda s, b: call(b, "GET", "/api/v1/me", token=access[other[s]]))

    print("[로그인 검증]")
    for label, body in [
        ("빈 본문", {}),
        ("이메일 형식 오류", {"email": "x", "password": "p", "affiliation": "aivance"}),
        ("타입 오류", {"email": 5, "password": "", "affiliation": "zz"}),
        ("소속 누락", {"email": "a@b.com", "password": "pw"}),
        ("비밀번호 불일치", {**admin, "password": "wrong-password"}),
        ("미가입 이메일", {**admin, "email": "nobody@example.com"}),
        ("다른 소속", {**admin, "affiliation": "aicura"}),
    ]:
        r.compare(f"POST /tokens {label}", lambda s, b, body=body: call(b, "POST", "/api/v1/tokens", body))
    r.compare("POST /tokens 잘못된 JSON", lambda s, b: call(b, "POST", "/api/v1/tokens", raw=b"{bad"))
    form = f"email={args.admin_email}&password=wrong&affiliation=aivance".encode()
    r.compare("POST /tokens 폼 인코딩", lambda s, b: call(b, "POST", "/api/v1/tokens", raw=form,
                                                     content_type="application/x-www-form-urlencoded"))

    print("[회원가입·인증]")
    base_reg = {"password": "Passw0rd!!x", "affiliation": "aicura", "name": "홍길동", "contact": "010-1234-5678",
                "terms_agreed": True, "third_party_agreed": True}
    for label, extra in [
        ("필수 누락", {"password": None, "terms_agreed": None}),
        ("약관 false", {"terms_agreed": False}),
        ("약한 비밀번호", {"password": "short"}),
        ("company 타입", {"company": 5}),
        ("프로필 미허용 키", {"profile": {"foo": 1, "age": -1}}),
        ("프로필 비객체", {"profile": "x"}),
        ("이름 공백", {"name": "   "}),
    ]:
        def reg(s, b, extra=extra):
            body = {**base_reg, "email": email(s, "v"), **extra}
            return call(b, "POST", "/api/v1/users", {k: v for k, v in body.items() if v is not None})
        r.compare(f"POST /users {label}", reg)
    reg_ok = r.compare("POST /users 성공", lambda s, b: call(b, "POST", "/api/v1/users", {
        **base_reg, "email": email(s, "reg"), "company": "회사", "profile": {"age": 30, "sex": "M"}}))
    r.compare("POST /users 중복 → 409", lambda s, b: call(b, "POST", "/api/v1/users", {**base_reg, "email": email(s, "reg")}))
    r.compare("미인증 로그인 → 403", lambda s, b: call(b, "POST", "/api/v1/tokens", {
        "email": email(s, "reg"), "password": base_reg["password"], "affiliation": "aicura"}))
    r.compare("POST /users/verify 누락", lambda s, b: call(b, "POST", "/api/v1/users/verify", {}))
    r.compare("POST /users/verify 무효", lambda s, b: call(b, "POST", "/api/v1/users/verify", {"token": "nope"}))
    r.compare("POST /users/verify/resend 소속 오류", lambda s, b: call(b, "POST", "/api/v1/users/verify/resend", {"email": "a@b.com", "affiliation": "x"}))
    r.compare("POST /users/verify/resend 성공", lambda s, b: call(b, "POST", "/api/v1/users/verify/resend", {"email": email(s, "reg"), "affiliation": "aicura"}))

    print("[로그 수집]")
    r.compare("POST /logs 성공", lambda s, b: call(b, "POST", "/api/v1/logs", {"level": "error", "message": "boom", "context": {"a": 1}, "user_id": 3, "logged_at": "2026-11-27T10:00:00+09:00"}))
    r.compare("POST /logs 검증 오류", lambda s, b: call(b, "POST", "/api/v1/logs", {"level": "x", "message": "m", "context": "s", "source": 3, "user_id": -1, "logged_at": "bad"}))
    r.compare("POST /logs 빈 본문", lambda s, b: call(b, "POST", "/api/v1/logs", {}))

    print("[운영자 계정 생성]")
    op_base = {"password": "Passw0rd!!x", "role": 2, "affiliation": "aivance", "name": "대행", "contact": "010"}
    r.compare("POST /operators 검증 오류", lambda s, b: call(b, "POST", "/api/v1/operators", {"role": "x"}, token=access[s]))
    r.compare("POST /operators role 2.0", lambda s, b: call(b, "POST", "/api/v1/operators", {**op_base, "role": 2.0, "email": email(s, "op")}, token=access[s]))
    r.compare("POST /operators 타 소속 → 403", lambda s, b: call(b, "POST", "/api/v1/operators", {**op_base, "affiliation": "aicura", "email": email(s, "op")}, token=access[s]))
    r.compare("POST /operators 성공", lambda s, b: call(b, "POST", "/api/v1/operators", {**op_base, "email": email(s, "op")}, token=access[s]))
    member = {s: login(b, {"email": email(s, "op"), "password": op_base["password"], "affiliation": "aivance"})["access_token"]
              for s, b in r.bases.items()}
    r.compare("비운영자 POST /operators → 403", lambda s, b: call(b, "POST", "/api/v1/operators", {}, token=member[s]))
    r.compare("비운영자 GET /users → 403", lambda s, b: call(b, "GET", "/api/v1/users", token=member[s]))

    print("[회원 관리]")
    for q in ["", "?per_page=1", "?q=" + stamp + "&sort=-email", "?role=2&is_active=1", "?sort=bad", "?role=9", "?is_active=yes"]:
        def lst(s, b, q=q):
            res = call(b, "GET", "/api/v1/users" + q.replace(stamp, f"{s}-{stamp}"), token=access[s])
            # 공유 DB 라 total 은 서버마다 같아야 한다 — 목록 항목 내용까지 비교
            return res
        r.compare(f"GET /users{q or ' (기본)'}", lst)
    admin_ids = {s: json.loads(call(b, "GET", "/api/v1/me", token=access[s])[2])["data"]["id"] for s, b in r.bases.items()}
    op_ids = {s: json.loads(call(b, "GET", "/api/v1/me", token=member[s])[2])["data"]["id"] for s, b in r.bases.items()}
    r.compare("GET /users/{admin}", lambda s, b: call(b, "GET", f"/api/v1/users/{admin_ids[s]}", token=access[s]))
    r.compare("GET /users/999999999 → 404", lambda s, b: call(b, "GET", "/api/v1/users/999999999", token=access[s]))
    r.compare("PATCH 본인 role → 403", lambda s, b: call(b, "PATCH", f"/api/v1/users/{admin_ids[s]}", {"role": 2}, token=access[s]))
    r.compare("PATCH 빈 본문 → 422", lambda s, b: call(b, "PATCH", f"/api/v1/users/{op_ids[s]}", {}, token=access[s]))
    r.compare("PATCH 검증 오류", lambda s, b: call(b, "PATCH", f"/api/v1/users/{op_ids[s]}", {"role": 7, "is_active": "maybe", "name": ""}, token=access[s]))
    r.compare("PATCH 성공", lambda s, b: call(b, "PATCH", f"/api/v1/users/{op_ids[s]}", {"name": "변경됨", "company": None, "role": "3", "is_active": True}, token=access[s]))

    print("[내 정보]")
    r.compare("GET /me", lambda s, b: call(b, "GET", "/api/v1/me", token=member[s]))
    r.compare("PATCH /me 빈 본문", lambda s, b: call(b, "PATCH", "/api/v1/me", {}, token=member[s]))
    r.compare("PATCH /me 비밀번호·현재비번 누락", lambda s, b: call(b, "PATCH", "/api/v1/me", {"password": "Newpassw0rd!"}, token=member[s]))
    r.compare("PATCH /me 현재비번 불일치", lambda s, b: call(b, "PATCH", "/api/v1/me", {"password": "Newpassw0rd!", "current_password": "nope"}, token=member[s]))
    r.compare("PATCH /me 프로필 미허용(aivance)", lambda s, b: call(b, "PATCH", "/api/v1/me", {"profile": {"age": 1}}, token=member[s]))
    r.compare("PATCH /me 성공", lambda s, b: call(b, "PATCH", "/api/v1/me", {"name": "새이름", "contact": "010-9", "company": "회사/株式"}, token=member[s]))
    # 본인 비밀번호 변경 — 다른 기기 세션 폐기 + 현재 기기용 새 토큰(data.tokens) 발급
    other_device = {s: login(b, {"email": email(s, "op"), "password": op_base["password"], "affiliation": "aivance"})["refresh_token"]
                    for s, b in r.bases.items()}
    new_pw = "Ch4nged!Passw0rd"
    r.compare("PATCH /me 비밀번호 변경 → tokens", lambda s, b: call(b, "PATCH", "/api/v1/me", {"password": new_pw, "current_password": op_base["password"]}, token=member[s]))
    r.compare("다른 기기 refresh → 401", lambda s, b: call(b, "POST", "/api/v1/tokens/refresh", {"refresh_token": other_device[s]}))
    r.compare("PATCH /me 비밀번호 원복", lambda s, b: call(b, "PATCH", "/api/v1/me", {"password": op_base["password"], "current_password": new_pw}, token=member[s]))

    print("[토큰 회전·로그아웃·탈퇴]")
    refresh = {s: login(b, {"email": email(s, "op"), "password": op_base["password"], "affiliation": "aivance"})["refresh_token"]
               for s, b in r.bases.items()}
    r.compare("POST /tokens/refresh 누락", lambda s, b: call(b, "POST", "/api/v1/tokens/refresh", {}))
    r.compare("POST /tokens/refresh 무효", lambda s, b: call(b, "POST", "/api/v1/tokens/refresh", {"refresh_token": "nope"}))
    r.compare("POST /tokens/refresh 성공", lambda s, b: call(b, "POST", "/api/v1/tokens/refresh", {"refresh_token": refresh[s]}))
    r.compare("POST /tokens/refresh 재사용 → 전체 무효화", lambda s, b: call(b, "POST", "/api/v1/tokens/refresh", {"refresh_token": refresh[s]}))
    # 정지된 계정은 refresh 로 세션을 연장할 수 없다 — 비활성화 시 리프레시 토큰 전체 폐기
    suspended = {s: login(b, {"email": email(s, "op"), "password": op_base["password"], "affiliation": "aivance"})["refresh_token"]
                 for s, b in r.bases.items()}
    r.compare("PATCH is_active=false (정지)", lambda s, b: call(b, "PATCH", f"/api/v1/users/{op_ids[s]}", {"is_active": False}, token=access[s]))
    r.compare("정지 계정 refresh → 401", lambda s, b: call(b, "POST", "/api/v1/tokens/refresh", {"refresh_token": suspended[s]}))
    r.compare("정지 계정 PATCH /me 비밀번호 → 403", lambda s, b: call(b, "PATCH", "/api/v1/me", {"password": "Ch4nged!Passw0rd", "current_password": op_base["password"]}, token=member[s]))
    r.compare("정지 계정 로그인 → 401", lambda s, b: call(b, "POST", "/api/v1/tokens", {"email": email(s, "op"), "password": op_base["password"], "affiliation": "aivance"}))
    r.compare("PATCH is_active=true (복구)", lambda s, b: call(b, "PATCH", f"/api/v1/users/{op_ids[s]}", {"is_active": True}, token=access[s]))
    r.compare("복구 후에도 폐기된 토큰은 무효", lambda s, b: call(b, "POST", "/api/v1/tokens/refresh", {"refresh_token": suspended[s]}))
    # 운영자 비밀번호 재설정 시 기존 리프레시 토큰 전체 폐기
    before_reset = {s: login(b, {"email": email(s, "op"), "password": op_base["password"], "affiliation": "aivance"})["refresh_token"]
                    for s, b in r.bases.items()}
    r.compare("PATCH password 재설정", lambda s, b: call(b, "PATCH", f"/api/v1/users/{op_ids[s]}", {"password": op_base["password"]}, token=access[s]))
    r.compare("재설정 전 refresh 토큰 → 401", lambda s, b: call(b, "POST", "/api/v1/tokens/refresh", {"refresh_token": before_reset[s]}))
    r.compare("DELETE /tokens 없는 토큰(멱등)", lambda s, b: call(b, "DELETE", "/api/v1/tokens", {"refresh_token": "nope"}))
    r.compare("DELETE /tokens 누락", lambda s, b: call(b, "DELETE", "/api/v1/tokens", {}))
    r.compare("DELETE /me 비밀번호 누락", lambda s, b: call(b, "DELETE", "/api/v1/me", {}, token=member[s]))
    r.compare("DELETE /me 비밀번호 불일치", lambda s, b: call(b, "DELETE", "/api/v1/me", {"password": "nope"}, token=member[s]))
    r.compare("DELETE /me 성공", lambda s, b: call(b, "DELETE", "/api/v1/me", {"password": op_base["password"]}, token=member[s]))
    r.compare("탈퇴 후 GET /me → 404", lambda s, b: call(b, "GET", "/api/v1/me", token=member[s]))

    print(f"\n결과: {r.passed} 통과 / {len(r.failed)} 불일치 / {len(r.known)} 알려진 차이")
    for name in r.failed:
        print(f"  - {name}")
    _ = reg_ok, re
    return 1 if r.failed else 0


if __name__ == "__main__":
    sys.exit(main())
