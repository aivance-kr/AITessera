---
title: The production server has no Rust toolchain; Rust is built in a separate job with no secrets and only the verified binary is shipped
author: Byunjongwon
date: 2026-11-27
branch: dev
cost: { minutes: 7, tokens: 49314 }
id: l-3e7ec0bc
scope: repo
status: active
replaces: l-df37a941
src: agent
touches:
  - ".github/workflows/deploy.yml"
  - CLAUDE.md
---

## Decided
- The production server never gets cargo/rustup. The Rust port is built on the self-hosted runner and only the binary is shipped.
- The build runs in its own job with no production secrets and no environment, because build.rs and proc-macros execute repository code during the build. It passes the binary as an artifact plus its sha256. The deploy job only verifies and uploads.
- The server installs the uploaded binary only when this run's sha256 matches. A leftover file from an earlier run is deleted, never installed. Only after that does it run ldd, so a binary needing a newer glibc than the server's is rejected.
- Both jobs run only on main, because the server always resets to origin/main. A manual run from another ref would ship a binary that does not match the deployed source.

## Tried and ruled out
- Building inside the deploy job: that job holds the SSH key, so the build would run code next to the secret. Ruled out after security review.
- appleboy/scp-action for the upload: it is a Docker container action and cannot see the workspace on our container runners. Plain scp is used instead.

## Watch out
- If the build job fails, the deploy job still runs and fails in its first step. That keeps the Slack failure alert. Don't make deploy plainly depend on success, or build failures go silent.
- SSH host key pinning is optional (DEPLOY_SSH_HOST_KEY). Without it the upload uses accept-new. The existing appleboy SSH step does not pin the host key either.
- No systemd unit exists for the Rust server yet, so the restart step only runs if aitessera-rust.service exists.

In the code: .github/workflows/deploy.yml jobs build-rust and deploy ("Rust 빌드 결과 확인", "Rust 바이너리 업로드 (.new)", SSH script step 6); RUST_BIN_REL = rust/target/release/aitessera; RUST_BIN_SHA256 from needs.build-rust.outputs.sha256.
