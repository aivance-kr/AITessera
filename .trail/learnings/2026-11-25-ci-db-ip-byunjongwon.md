---
title: CI는 러너가 컨테이너든 호스트든 DB를 컨테이너 IP로 접속한다
author: Byunjongwon
date: 2026-11-25
branch: fix/ci-runner-host-network
cost: { minutes: 1, tokens: 93038 }
id: l-962e3149
scope: repo
status: active
src: agent
touches:
  - ".github/workflows/ci.yml"
---

## Decided
러너 종류가 섞여 있어(일부는 도커 컨테이너, 일부는 호스트) 컨테이너명 접속에 의존하지 않는다. 러너가 컨테이너일 때만 CI 네트워크에 붙이고, DB/Redis는 네트워크 IP로 접속한다.

## Tried and ruled out
컨테이너명으로 접속 + 항상 `docker network connect "$(hostname)"`: 호스트 러너(martin-server)에서 `No such container`로 실패.

## Watch out
러너가 컨테이너라는 가정을 다른 워크플로에 복사하지 말 것.

In the code: .github/workflows/ci.yml 의 "MySQL · Redis 컨테이너 기동" 스텝, CI_MYSQL_ADDR / CI_REDIS_ADDR.
