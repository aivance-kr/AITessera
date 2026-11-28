---
title: "Behind Apache the Rust server trusts only the last X-Forwarded-For value, and only from loopback"
author: Byunjongwon
date: 2026-11-28
branch: dev
cost: { minutes: 3, tokens: 28752 }
id: l-8a57d1ba
scope: repo
status: active
replaces: l-9c709aca
src: agent
touches:
  - docs/2026-11-28-rust-production-cutover.md
  - rust/README.md
  - rust/src/http.rs
---

## Decided
- In production Apache proxies to the Rust server on the same host, so the socket peer is always loopback. Client IP for rate limiting and login events is the last X-Forwarded-For value, read only when the peer is loopback. Apache appends the real client IP at the end, so the last value cannot be forged; earlier values come from the client.
- A non-loopback peer, or a last value that is not a bare IP, falls back to the socket address, the same as PHP's REMOTE_ADDR. PHP keeps REMOTE_ADDR only because it runs inside Apache and already sees the real IP.
- The Rust server runs as a systemd service whose unit file is versioned in the repo and only linked on the server (`systemctl link`), so its content changes only through git. Stop signal is SIGINT because the server shuts down gracefully only on Ctrl-C. APP_TIMEZONE is pinned to UTC to match production PHP.

## Tried and ruled out
- Taking the first X-Forwarded-For value: clients can set it, so anyone could pick their rate-limit bucket or fake login IPs.
- Writing the unit directly under /etc/systemd/system: it drifts from the repo with no review.

## Watch out
- Any new code that needs the client IP must go through the shared resolver; reading ConnectInfo directly gives 127.0.0.1 in production.
- A local process on the server can send X-Forwarded-For straight to 127.0.0.1:8306 and choose its IP. Acceptable because only local users can reach that port.
- After a deploy that changes the unit file, run `sudo systemctl daemon-reload`; the deploy workflow does not.

In the code:
- rust/src/http.rs resolve_client_ip, client_ip, token_issue
- deploy/aitessera-rust.service; `sudo systemctl link /home/aitessera/AITessera/deploy/aitessera-rust.service`
- runbook: docs/2026-11-28-rust-production-cutover.md
