# 2026년 11월 AITessera 작업일지

- 대상 기간: 2026-11-01 ~ 2026-11-30 (KST)
- 집계: 닫힌 이슈 0건 · 이슈 없는 PR 4건 · 배포 4회
- 생성일: 2026-11-06

## 월간 요약

사이트 루트(`/`) 접속 시 401 메시지만 보이던 문제를 해결하려고 공개 소개 페이지를 추가했다.
이후 소개 페이지의 JWKS 링크를 서버 환경에 맞게 수정하고, SEO·GEO(메타 태그, JSON-LD, robots.txt, sitemap.xml, llms.txt)를 적용했으며, 로고·파비콘을 추가했다.
이달 작업은 전부 2일에 이루어졌고 PR 4건이 각각 `dev → main` 배포 PR로 반영됐다. 닫힌 이슈는 없다.

## 일자별 작업

### 2026-11-02 (월)

- **[PR #87]** ✨ feat: 루트(/) 소개 페이지 추가 — 루트 접속 시 401만 표시되던 문제를 `GET /` 공개 라우트와 `LandingController`, `resources/landing.html`(개요·아키텍처·기술·API 목록, 라이트/다크, 모바일 대응)로 해결하고 `PipelineTest`에 공개 접근 테스트 추가 ([링크](https://github.com/aivance-kr/AITessera/pull/87))
- **[PR #89]** 🐛 fix: 소개 페이지 JWKS 링크를 버전 경로로 변경 — 서버 Apache가 `/.well-known`을 403으로 막아 `/api/v1/jwks.json`으로 교체 ([링크](https://github.com/aivance-kr/AITessera/pull/89))
- **[PR #91]** ✨ feat: 소개 페이지 SEO·GEO 적용 — 메타·OG·JSON-LD 4종·FAQ 섹션 추가, `SeoController`로 `/robots.txt`·`/sitemap.xml`·`/llms.txt` 제공, `APP_BASE_URL` 기반 절대 URL 치환, 테스트 추가 ([링크](https://github.com/aivance-kr/AITessera/pull/91))
- **[PR #93]** ✨ feat: 로고·파비콘 추가 — 파비콘(svg/ico)·apple-touch-icon·512 아이콘·워드마크 로고 추가, `/api/docs` 페이지에 파비콘 연결 ([링크](https://github.com/aivance-kr/AITessera/pull/93))
- **[배포 PR #88]** dev → main 배포 — 루트 소개 페이지 ([링크](https://github.com/aivance-kr/AITessera/pull/88))
- **[배포 PR #90]** dev → main 배포 — JWKS 링크 수정 ([링크](https://github.com/aivance-kr/AITessera/pull/90))
- **[배포 PR #92]** dev → main 배포 — 홈 화면 SEO·GEO 적용 ([링크](https://github.com/aivance-kr/AITessera/pull/92))
- **[배포 PR #94]** dev → main 배포 — 로고·파비콘 추가 ([링크](https://github.com/aivance-kr/AITessera/pull/94))
