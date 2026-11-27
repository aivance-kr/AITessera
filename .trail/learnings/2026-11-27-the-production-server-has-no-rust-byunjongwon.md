---
title: The production server has no Rust toolchain; the deploy job builds the Rust binary on the runner and uploads it
author: Byunjongwon
date: 2026-11-27
branch: feature/deploy-rust-build
cost: { minutes: 3, tokens: 41598 }
id: l-df37a941
scope: repo
status: active
src: agent
touches:
  - ".github/workflows/deploy.yml"
  - CLAUDE.md
---

## Decided
- The production server does not get cargo/rustup. The deploy job builds the Rust port (`cargo build --release --locked`) on the self-hosted runner and uploads only the binary, so the server never compiles anything and a build failure stops the deploy before the server is touched.
- Upload uses a plain `scp` shell step, not appleboy/scp-action: that is a Docker container action and cannot see the workspace on our container runners.
- The binary lands as `.new` and is swapped with `mv -f` on the server after an `ldd` check, so a binary linked against a newer glibc than the server's is rejected instead of breaking the service.

## Watch out
- Runner images differ (glibc may differ from the server's Ubuntu 24.04 / glibc 2.39). The ldd check is the guard; if it fires, build on an older base image rather than upgrading the server.
- There is no systemd unit for the Rust server yet; the restart step only runs if `aitessera-rust.service` exists.

In the code: .github/workflows/deploy.yml steps "Rust release 빌드", "Rust 바이너리 업로드 (.new)", SSH script step 6; RUST_BIN_REL = rust/target/release/aitessera.
