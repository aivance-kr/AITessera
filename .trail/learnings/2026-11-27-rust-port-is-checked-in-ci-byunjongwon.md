---
title: "Rust port is checked in CI with rustfmt, Clippy and unit tests only, no database"
author: Byunjongwon
date: 2026-11-27
branch: feature/ci-rust-checks
cost: { minutes: 3, tokens: 99173 }
id: l-5a0e9782
scope: repo
status: active
src: agent
touches:
  - ".github/workflows/ci.yml"
  - CLAUDE.md
  - rust/src/config.rs
  - rust/src/domain.rs
  - rust/src/http.rs
  - rust/src/infra.rs
  - rust/src/jwt.rs
  - rust/src/main.rs
  - rust/src/php.rs
  - rust/src/repo.rs
---

## Decided
- The Rust port gets its own CI job next to the PHP backend job: rustfmt check, Clippy with warnings as errors, and cargo test. It runs only unit tests, so no MySQL/Redis containers.
- The existing Rust code had never been run through rustfmt; it was reformatted in a separate style-only commit before the check was turned on.

## Watch out
- The Rust source exists on dev (since PR #99) but not on main. Branch Rust work from dev, not main.
- Run cargo fmt before pushing Rust changes, or the deploy PR CI fails on formatting.

In the code:
- .github/workflows/ci.yml job `rust` (working-directory rust/)
- commands: cargo fmt --all -- --check; cargo clippy --all-targets --locked -- -D warnings; cargo test --locked
