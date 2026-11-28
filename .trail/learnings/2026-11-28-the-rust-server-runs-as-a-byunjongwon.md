---
title: The Rust server runs as a systemd service whose unit file lives in the repo and is only linked on the server
author: Byunjongwon
date: 2026-11-28
branch: dev
cost: { minutes: 4, tokens: 60310 }
id: l-9c709aca
scope: repo
status: active
src: agent
touches:
  - deploy/aitessera-rust.service
  - docs/2026-11-28-rust-production-cutover.md
  - rust/src/http.rs
---

## Decided
- The unit file is versioned in the repository and attached on the production server with `systemctl link`, so its content changes only through git and the deploy path. Only the link and one sudoers line live outside the repo.
- The unit sends SIGINT on stop, because the Rust server only shuts down gracefully on Ctrl-C; the default SIGTERM would cut in-flight requests.
- APP_TIMEZONE is pinned to UTC in the unit because production PHP (CLI and FPM) runs in UTC; the two must match or DATETIME strings drift.

## Tried and ruled out
- Writing the unit directly under /etc/systemd/system: it would drift from the repo with no review, against the rule of keeping config in the repository.

## Watch out
- After a deploy that changes the unit file, run `sudo systemctl daemon-reload` on the server; the deploy workflow does not.
- Behind the Apache proxy the Rust server sees 127.0.0.1 for every client, so rate limits become global and login events lose real IPs. Do not switch traffic until client IP is taken from the last X-Forwarded-For value, trusted only from a loopback peer, in both places that read the IP.

In the code:
- deploy/aitessera-rust.service; `sudo systemctl link /home/aitessera/AITessera/deploy/aitessera-rust.service`
- sudoers: `<DEPLOY_USER> ALL=(ALL) NOPASSWD: /usr/bin/systemctl restart aitessera-rust`
- IP readers: rust/src/http.rs client_ip() and token_issue (ConnectInfo)
- runbook: docs/2026-11-28-rust-production-cutover.md
