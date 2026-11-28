---
title: "PHP and Rust are benchmarked from inside the production server over loopback, within the rate limits"
author: Byunjongwon
date: 2026-11-27
branch: docs/php-rust-benchmark
cost: { minutes: 3, tokens: 42950 }
id: l-bc56b76a
scope: repo
status: active
src: agent
touches:
  - docs/2026-11-27-php-rust-performance.md
  - ".github/workflows/deploy.yml"
  - CLAUDE.md
---

## Decided
- Production PHP-vs-Rust comparisons run on the production server itself, calling both over loopback, so neither side pays a network hop the other doesn't. The Rust server listens only on 127.0.0.1:8306 and is not behind Apache yet, so this is also the only fair way to reach it.
- A static file on the same vhost over https is measured alongside, to separate TLS/Apache cost from PHP app cost (static ~0.2ms vs PHP /health ~3.8ms).
- Production rate limits (API 120/min, auth 10/min per IP) were left untouched. Only /health (exempt) gets a throughput test; DB/auth scenarios are measured at concurrency 1 within the limits (me+users ≤ 120 per minute, login ≤ 9). Rust uses separate ratelimit:rust:* keys, so each server gets its own budget.
- The admin credentials are read from the server's own .env inside the remote script; they never leave the server.

## Tried and ruled out
- Hitting PHP via the public URL and Rust via an SSH tunnel: different network paths, so the numbers compare networks, not implementations.

## Watch out
- `ssh prod` IS the tessera server (vhost is in 000-default-le-ssl.conf, not a tessera-named file).
- To tell which implementation answers a URL, send a multipart/form-data login: PHP parses it (INVALID_CREDENTIALS), Rust treats it as empty input (VALIDATION_ERROR).
- curl to https://127.0.0.1 with only a Host header goes to the wrong vhost (SNI mismatch) and returns non-JSON; use --resolve tessera.aivance.kr:443:127.0.0.1. ab with -H Host worked.
- ab -e percentile CSV gives garbage p99 for small runs (50 or fewer requests); read p50/p90/mean only.
- The logs scenario pushes "bench" messages into the production Redis log queue; logins create refresh tokens.

In the code: results and reproduction commands in docs/2026-11-27-php-rust-performance.md; ab is installed on the server; PHP-FPM 8.5 pool max_children=150, OPcache+JIT on.
