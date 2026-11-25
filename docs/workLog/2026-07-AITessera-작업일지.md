# 2026년 7월 AITessera 작업일지

- 대상 기간: 2026-07-01 ~ 2026-07-31 (KST)
- 집계: 닫힌 이슈 21건 · 이슈 없는 PR 14건 · 배포 12회
- 생성일: 2026-11-06

## 월간 요약

프로젝트 시작 달. 5일 순수 PHP 마이크로 스택 스캐폴딩, CI, DI·미들웨어 파이프라인을 올리고 회원가입·이메일 인증, 소속별 profile, RateLimit, Swagger UI, 로그 수집 파이프라인을 구현했다.
6~8일에는 운영자용 회원 관리 API, 회원 셀프서비스(수정·탈퇴), 회원구분, SSH 자동 배포(`deploy.yml`)를 추가하고 JWT를 RS256으로 바꿔 공개키(JWKS)를 외부에 제공했다.
13~16일에는 AI 로그 분석·운영 자동화(분류·이상탐지, 일일 리포트, 로그인 이상 탐지)와 배포 후 Slack 알림을 넣었다. 30일에는 동일 이메일 다중 소속 가입 지원과 검증 게이트 정책(CI는 `dev → main` PR에서만)을 적용했다.

## 일자별 작업

### 2026-07-05 (일)

- **[이슈 #1]** db 구조 — users DB 구조 요구사항 정리 — 이메일·비밀번호·소속, 이메일 인증, 약관/제3자 정보제공 동의, 소속별 JSON 컬럼 ([링크](https://github.com/aivance-kr/AITessera/issues/1))
- **[이슈 #7]** feat: 회원가입 엔드포인트 + 이메일 인증 발송 — 회원가입 `POST /api/v1/users`와 이메일 인증 발송 구현 ([링크](https://github.com/aivance-kr/AITessera/issues/7))
- **[이슈 #8]** feat: 소속별 profile(JSON) 스키마 정의·검증 — 소속별 `users.profile`(JSON) 필드 스키마 정의와 검증 ([링크](https://github.com/aivance-kr/AITessera/issues/8))
- **[이슈 #9]** feat: RateLimit 미들웨어 (brute-force 방어) — `symfony/rate-limiter` + Redis 기반 RateLimit 미들웨어로 로그인 brute-force 방어 ([링크](https://github.com/aivance-kr/AITessera/issues/9))
- **[이슈 #10]** feat: Swagger UI 노출 (/api/docs) — `zircote/swagger-php`로 OpenAPI 스펙을 만들고 `/api/docs`에 Swagger UI 노출 ([링크](https://github.com/aivance-kr/AITessera/issues/10))
- **[이슈 #11]** feat: 로그 수집 파이프라인 (큐 컨슈머) — `POST /api/v1/logs` → Redis 큐 적재 후 202 응답, `bin/console` 큐 컨슈머로 로그 수집 파이프라인 구현 ([링크](https://github.com/aivance-kr/AITessera/issues/11))
- **[이슈 #13]** 회원 추가 필드들 — 소속별 회원 추가 필드 정의 (aicopia·aicreo·aicura 등 이름 필드와 추가 필드) ([링크](https://github.com/aivance-kr/AITessera/issues/13))
- **[PR #2]** chore: 순수 PHP 마이크로 스택 프로젝트 스캐폴딩 초기 구성 — 프레임워크 없는 순수 모던 PHP(8.4+) 마이크로 스택 프로젝트 초기 스캐폴딩 ([링크](https://github.com/aivance-kr/AITessera/pull/2))
- **[PR #3]** ci: GitHub Actions CI 워크플로우 추가 — `dev`·`main` 대상 push/PR 자동 검증 GitHub Actions CI 워크플로우 추가 ([링크](https://github.com/aivance-kr/AITessera/pull/3))
- **[PR #4]** feat: DI 컨테이너 + PSR-15 미들웨어 파이프라인 부트스트랩 — DI 컨테이너와 PSR-15 미들웨어 파이프라인(프론트 컨트롤러 → 컨테이너 → Relay → 컨트롤러) 부트스트랩 ([링크](https://github.com/aivance-kr/AITessera/pull/4))
- **[PR #17]** docs: 엔드포인트 OpenAPI requestBody 추가 (Swagger UI 입력란) — Swagger UI에서 로그인 등 입력란이 안 보이던 문제를 엔드포인트 OpenAPI `requestBody` 추가로 해결 ([링크](https://github.com/aivance-kr/AITessera/pull/17))

### 2026-07-06 (월)

- **[이슈 #23]** api 정보 확인 — `/api/v1/me` 응답 정보가 적은 이유(DB 정보 부재 여부·의도 여부) 확인 요청 ([링크](https://github.com/aivance-kr/AITessera/issues/23))
- **[이슈 #26]** qc 테스트 케이스 작성 — 가입 → 이메일 인증 → 로그인 → 내 정보 → 리프레시 토큰 재발급 시나리오로 QC 테스트 케이스 작성·진행 ([링크](https://github.com/aivance-kr/AITessera/issues/26))
- **[이슈 #29]** 회원구분 추가 — 회원구분(운영자=1, 대행사=2, 일반회원=3) 추가, 운영자 페이지 접근·토큰 인증 체크에 반영 ([링크](https://github.com/aivance-kr/AITessera/issues/29))
- **[이슈 #33]** 🚀 배포 사전 준비: 프로덕션 .env · 아파치 DocumentRoot(public/) · var/ 쓰기권한(setgid) — 운영 서버 최초 1회 배포 사전 준비 — 프로덕션 .env, Apache DocumentRoot(`public/`), `var/` 쓰기 권한 ([링크](https://github.com/aivance-kr/AITessera/issues/33))
- **[이슈 #34]** 운영자용 api 추가 — 운영자용 API 추가 — 회원 리스트, 회원 상세(롤 포함), 대행사/회원 생성·수정 ([링크](https://github.com/aivance-kr/AITessera/issues/34))
- **[PR #19]** fix: UserRepository 중복 named placeholder (HY093) — 로그인·회원가입 실패 수정 — `UserRepository`의 중복 named placeholder로 로그인·회원가입·이메일 인증이 실패(HY093)하던 문제 수정 ([링크](https://github.com/aivance-kr/AITessera/pull/19))
- **[PR #20]** docs: 로컬 셋업 문서 보강 (Redis·MySQL·마이그레이션·시더) — README 로컬 환경 설정에 Redis·MySQL·마이그레이션·시더 절차 보강 ([링크](https://github.com/aivance-kr/AITessera/pull/20))
- **[PR #21]** docs: OpenAPI 문서 전면 보강 — 재사용 스키마 컴포넌트 + 토큰 종류 명확화 — OpenAPI 문서를 재사용 스키마 컴포넌트(9종)로 정리하고 토큰 종류 설명을 명확화 ([링크](https://github.com/aivance-kr/AITessera/pull/21))
- **[PR #28]** 📝 docs: API 문서 버전 1.0.0 + 앱 버전 단일 출처 일원화 — OpenAPI 문서 버전을 1.0.0으로 올리고 앱 버전을 `Version::APP_VERSION` 단일 출처로 일원화 ([링크](https://github.com/aivance-kr/AITessera/pull/28))
- **[PR #32]** 👷 chore: 프로덕션 SSH 자동 배포 워크플로우(deploy.yml) 추가 — `main` push 시 SSH로 프로덕션 자동 배포하는 `deploy.yml` 워크플로우 추가 ([링크](https://github.com/aivance-kr/AITessera/pull/32))
- **[배포 PR #22]** dev → main 배포 — Dev ([링크](https://github.com/aivance-kr/AITessera/pull/22))
- **[배포 PR #31]** dev → main 배포 — 🚀 deploy: dev → main (회원구분/UserRole 포함) ([링크](https://github.com/aivance-kr/AITessera/pull/31))

### 2026-07-07 (화)

- **[이슈 #39]** 회원 api 추가 — 회원 본인 정보 수정 API(토큰으로 본인 확인)와 회원 탈퇴 API 추가 ([링크](https://github.com/aivance-kr/AITessera/issues/39))
- **[배포 PR #36]** dev → main 배포 — 🚀 deploy: dev → main 배포 (운영자 회원관리 API · CD 워크플로우) ([링크](https://github.com/aivance-kr/AITessera/pull/36))
- **[배포 PR #38]** dev → main 배포 — 🚀 deploy: OpenAPI 스펙 캐시 무효화 핫픽스 ([링크](https://github.com/aivance-kr/AITessera/pull/38))

### 2026-07-08 (수)

- **[이슈 #41]** jwt 비대칭키로 변경 — JWT 서명 알고리즘을 HS256에서 RS256(개인키 서명)으로 변경 ([링크](https://github.com/aivance-kr/AITessera/issues/41))
- **[이슈 #45]** JWT 검증에 StrictValidAt 미적용 — iat/nbf 미검증 (문서-코드 드리프트) — JWT 검증에 `StrictValidAt`이 적용되지 않아 iat/nbf가 검증되지 않는 문서-코드 불일치 처리 ([링크](https://github.com/aivance-kr/AITessera/issues/45))
- **[이슈 #46]** 비대칭키 공개키를 외부에서 자동확인 — 비대칭키 공개키를 외부에서 자동 확인할 수 있도록 변경 ([링크](https://github.com/aivance-kr/AITessera/issues/46))
- **[배포 PR #43]** dev → main 배포 — 🚀 deploy: dev → main (JWT RS256 + 회원 셀프서비스) ([링크](https://github.com/aivance-kr/AITessera/pull/43))

### 2026-07-13 (월)

- **[이슈 #49]** ✨ AI 기반 로그 분석·운영 자동화 (Epic) — 로그 수집 파이프라인과 인증 인프라를 활용한 AI 로그 분석·운영 자동화 Epic ([링크](https://github.com/aivance-kr/AITessera/issues/49))
- **[이슈 #50]** ✨ 로그 자동 분류·요약·이상탐지 (AI) — `client_logs` 저장 전 AI로 로그 자동 분류·요약·이상탐지 ([링크](https://github.com/aivance-kr/AITessera/issues/50))
- **[이슈 #51]** ✨ 로그 기반 일일 이상징후 리포트 (AI) — 하루치 `client_logs`를 AI로 분석해 일일 이상징후 리포트를 만들고 메일 큐로 발송 ([링크](https://github.com/aivance-kr/AITessera/issues/51))
- **[이슈 #52]** 🔒 로그인 이상 탐지 (AI 보안) — 로그인 시도 패턴을 AI로 스코어링해 비정상 로그인 탐지 ([링크](https://github.com/aivance-kr/AITessera/issues/52))
- **[PR #54]** 🔧 chore: 검증 루프 가속 — 개발용 fix 스크립트 + pre-push 훅 — 검증 루프 가속 — `composer fix` 스크립트와 pre-push 훅 추가 ([링크](https://github.com/aivance-kr/AITessera/pull/54))

### 2026-07-14 (화)

- **[배포 PR #60]** dev → main 배포 — 🚀 deploy: dev → main (AI 로그 분석·운영 자동화 + 로그인 이상 알림/리포트) ([링크](https://github.com/aivance-kr/AITessera/pull/60))

### 2026-07-16 (목)

- **[PR #61]** ✨ feat: 배포 완료 후 Slack 알림 추가 — `deploy.yml` 배포 완료 후 성공/실패 Slack Incoming Webhook 알림 추가 ([링크](https://github.com/aivance-kr/AITessera/pull/61))
- **[배포 PR #62]** dev → main 배포 — 🚀 deploy: dev → main (배포 완료 후 Slack 알림) ([링크](https://github.com/aivance-kr/AITessera/pull/62))
- **[배포 PR #64]** dev → main 배포 — 🚀 deploy: dev → main (Slack 알림 if 조건 오류 핫픽스) ([링크](https://github.com/aivance-kr/AITessera/pull/64))
- **[배포 PR #66]** dev → main 배포 — 🚀 deploy: dev → main (Slack 배포 알림 메시지 포맷 개선) ([링크](https://github.com/aivance-kr/AITessera/pull/66))
- **[배포 PR #68]** dev → main 배포 — 🚀 deploy: dev → main (Slack 배포 알림 작업내용 링크 추가) ([링크](https://github.com/aivance-kr/AITessera/pull/68))
- **[배포 PR #71]** dev → main 배포 — 🚀 deploy: 동일 이메일 다중 소속 가입 지원 ([링크](https://github.com/aivance-kr/AITessera/pull/71))

### 2026-07-30 (목)

- **[이슈 #69]** ✨ 동일 이메일로 여러 제품군(소속)에 독립 계정 가입 지원 — 같은 이메일로 서로 다른 소속(affiliation)에 독립 계정 가입·로그인 지원 ([링크](https://github.com/aivance-kr/AITessera/issues/69))
- **[PR #72]** ✨ feat: 검증 게이트 정책 적용 — CI는 dev→main PR에서만 — 검증 게이트 정책 반영 — `ci.yml` 트리거를 `dev → main` PR로 한정, `feature → dev`는 로컬 훅이 게이트 ([링크](https://github.com/aivance-kr/AITessera/pull/72))
- **[PR #75]** 🔧 chore: 로컬 개발 포트 8305로 변경 — 로컬 개발 포트를 9300에서 8305로 변경 (`.test` 도메인 동시 구동 작업의 일환) ([링크](https://github.com/aivance-kr/AITessera/pull/75))
- **[PR #76]** 🐛 fix: composer serve 300초 타임아웃으로 개발 서버 자동 종료되는 문제 수정 — Composer 스크립트 기본 타임아웃(300초)으로 `composer serve` 개발 서버가 종료되던 문제 수정 ([링크](https://github.com/aivance-kr/AITessera/pull/76))
- **[배포 PR #74]** dev → main 배포 — 🚀 deploy: 검증 게이트 정책 적용 — 로컬 훅 게이트 + self-hosted CI ([링크](https://github.com/aivance-kr/AITessera/pull/74))
