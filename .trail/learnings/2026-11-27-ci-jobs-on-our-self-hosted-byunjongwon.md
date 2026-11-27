---
title: "CI jobs on our self-hosted runner never run pull requests from forks, and actions are pinned to commit hashes"
author: Byunjongwon
date: 2026-11-27
branch: feature/ci-backend-hardening
cost: { minutes: 7, tokens: 35658 }
id: l-3a10d59e
scope: repo
status: active
src: agent
touches:
  - ".github/workflows/ci.yml"
  - ".github/workflows/deploy.yml"
  - CLAUDE.md
  - rust/src/config.rs
  - rust/src/domain.rs
  - rust/src/http.rs
  - rust/src/infra.rs
  - rust/src/jwt.rs
  - rust/src/main.rs
  - rust/src/php.rs
---

## Decided
- The repo is public and CI runs PR code on the organization's self-hosted runner, so every CI job that executes PR content must only run for PRs whose branch lives in this repository. A fork PR would otherwise run arbitrary code (composer scripts, PHPUnit, Rust build scripts/tests) on the shared runner.
- Third-party actions are pinned to full commit hashes with the version as a trailing comment, so a moved or tampered tag cannot change what runs. The pinned hash is whatever the old major tag pointed to, so pinning itself changes no behavior.

## Watch out
- Any new CI job that checks out and runs PR code needs the same fork guard.
- Bumping an action means replacing the hash by hand (look it up with gh api) until Dependabot for GitHub Actions is set up.
- The deploy workflow only triggers on pushes to main, so it has no fork guard; it does not need one.

In the code:
- guard on each job in .github/workflows/ci.yml: if: github.event.pull_request.head.repo.full_name == github.repository
- hash lookup: gh api repos/<owner>/<repo>/commits/<tag> --jq .sha
