# 2026년 10월 AITessera 작업일지

- 대상 기간: 2026-10-01 ~ 2026-10-31 (KST)
- 집계: 닫힌 이슈 0건 · 이슈 없는 PR 2건 · 배포 1회
- 생성일: 2026-11-06

## 월간 요약

self-hosted 도커 러너 환경에 맞춘 CI 수정이 이달 작업의 전부다.
워크플로 `runs-on` 라벨에 `docker`를 추가했고, 도커 컨테이너 액션이 러너 작업 디렉터리를 찾지 못해 실패하던 커버리지 요약 단계를 셸 스크립트로 바꿨다.
작업은 모두 5일에 이루어졌고 `dev → main` 배포 PR 1건으로 반영됐다. 닫힌 이슈는 없다.

## 일자별 작업

### 2026-10-05 (월)

- **[PR #82]** 🔧 chore: CI 러너 라벨에 docker 추가 — self-hosted Linux 러너를 쓰는 워크플로의 `runs-on`을 `[self-hosted, Linux, X64, docker]`로 변경, AILign 수정 내용을 동일 반영 ([링크](https://github.com/aivance-kr/AITessera/pull/82))
- **[PR #85]** 🐛 fix: CI 커버리지 마크다운을 도커 액션 대신 셸로 생성 — 러너가 도커 컨테이너라 `irongut/CodeCoverageSummary`가 `No files found matching glob pattern`으로 실패해, 기존 `build/coverage.txt`로 `code-coverage-results.md`를 셸에서 직접 생성 ([링크](https://github.com/aivance-kr/AITessera/pull/85))
- **[배포 PR #86]** dev → main 배포 ([링크](https://github.com/aivance-kr/AITessera/pull/86))
