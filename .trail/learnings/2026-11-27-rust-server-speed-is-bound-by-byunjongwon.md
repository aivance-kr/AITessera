---
title: "Rust server speed is bound by database round trips, not CPU; cut round trips, skip allocator tricks"
author: Byunjongwon
date: 2026-11-27
branch: feature/rust-perf-idiomatic
cost: { minutes: 18, tokens: 269384 }
id: l-87e2d83d
scope: repo
status: active
src: agent
touches:
  - rust/src/response.rs
  - rust/README.md
  - rust/src/main.rs
  - rust/src/service.rs
  - rust/src/infra.rs
---

## Decided
- The Rust port is I/O-bound: a primary-key query cost ~0.28ms while the in-app CPU (RS256 verify, JSON) was ~10-20µs. Gains came only from removing DB round trips.
- sqlx pools PING every connection on acquire by default (test_before_acquire=true), doubling round trips for single-query requests. It is turned off; idle_timeout 5min / max_lifetime 30min retire connections well before MySQL wait_timeout.
- Independent reads run concurrently with tokio::join!. The operator check runs concurrently with the data query using the token's `aff` claim as a hint; the operator result is judged first and a mismatched hint re-queries with the DB affiliation, so responses equal the sequential version.
- Argon2 hashing is bounded by a semaphore = CPU cores (burst throughput +14%, no 64MiB-per-thread blowup).

## Tried and ruled out
- mimalloc global allocator: within noise on every endpoint, and it kept Argon2's 64MiB work buffers after a login burst (RSS 7MB -> 670MB).
- Reusing a pooled Argon2 memory buffer (hash_password_into_with_memory): no latency change on macOS, more resident memory.
- Typed Serialize responses instead of serde_json::Value: kept for fewer allocations, but measured gain is ~0-3%; don't expect more from further serialization work.

## Watch out
- Running cargo builds during a benchmark wrecks the numbers (rps fell 3x on both servers). Benchmark old vs new binaries alternately with nothing else running.
- In zsh, a loop variable named `path` overwrites PATH.

In the code: rust/src/main.rs (pool options), rust/src/service.rs (list_users/get_user/login joins), rust/src/infra.rs (HASH_PERMITS), rust/README.md "Rust 방식 최적화".
