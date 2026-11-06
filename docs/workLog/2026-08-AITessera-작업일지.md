# 2026년 8월 AITessera 작업일지

- 대상 기간: 2026-08-01 ~ 2026-08-31 (KST)
- 집계: 닫힌 이슈 0건 · 이슈 없는 PR 3건 · 배포 0회
- 생성일: 2026-11-06

## 월간 요약

이슈 없이 PR 3건만 머지됐고 배포는 없었다. CI 컨테이너 정리 시 익명 볼륨 제거(14일), 로컬 개발 서버의 IPv6 바인딩 수정(19일), 아키텍처·로그인 시퀀스 다이어그램 문서 추가(29일)가 전부다.

## 일자별 작업

### 2026-08-14 (금)

- **[PR #77]** 🔧 chore: CI 컨테이너 정리 시 익명 볼륨까지 제거 — CI 컨테이너 정리 시 `docker rm -f -v`로 익명 볼륨까지 제거 (AICreo에서 볼륨 210개·44GB 누적으로 MySQL 기동 실패) ([링크](https://github.com/aivance-kr/AITessera/pull/77))

### 2026-08-19 (수)

- **[PR #78]** 🐛 fix: composer serve가 IPv6 전용으로 바인딩되는 문제 수정 — `composer serve`가 IPv6(`::1`)로만 리슨되어 Caddy 프록시가 502를 반환하던 문제 수정 ([링크](https://github.com/aivance-kr/AITessera/pull/78))

### 2026-08-29 (토)

- **[PR #79]** 📝 docs: 아키텍처·로그인 시퀀스 다이어그램 추가 — archify로 만든 아키텍처·로그인 토큰 발급 시퀀스 다이어그램 2종을 `docs/`에 추가하고 README에 링크 ([링크](https://github.com/aivance-kr/AITessera/pull/79))
