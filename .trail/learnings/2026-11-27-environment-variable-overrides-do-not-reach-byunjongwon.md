---
title: Environment variable overrides do not reach PHP config; run against a test DB with variables_order=EGPCS
author: Byunjongwon
date: 2026-11-27
branch: claude/php-rust-performance-test-2a1a02
cost: { minutes: 36, tokens: 620778 }
id: l-be4725a8
scope: repo
status: active
src: agent
touches:
  - rust/.gitignore
  - rust/Cargo.lock
  - rust/Cargo.toml
  - rust/README.md
  - rust/scripts/bench.sh
  - rust/scripts/dump_openapi.php
  - rust/scripts/parity_check.py
  - rust/src/config.rs
  - rust/src/domain.rs
  - rust/src/error.rs
---

## Decided
To point PHP at another DB/key path (e.g. a benchmark or test DB), pass `-d variables_order=EGPCS` to every PHP process. Without it, exported env vars are silently ignored and the values from `.env` are used.
- PDO runs with native prepares (EMULATE_PREPARES=false), which rejects reusing one named placeholder twice in a query (HY093, surfaced as 500). Give each occurrence its own name even when the value is the same.
- A suspended (is_active=0) account cannot extend its session: refresh revokes all its refresh tokens and returns INVALID_TOKEN. Deactivating a member through the admin PATCH also revokes all their refresh tokens in the same transaction. Access tokens still live until expiry (max 15 min).
- PHP and the Rust port must change together so the parity check stays at 0 mismatches.
- When an admin resets a member's password (PATCH /users/{id} with password), all of that member's refresh tokens are revoked in the same transaction, the same as deactivation. A leaked account can't keep its session after an admin locks it out.

## Tried and ruled out
- `export DB_NAME=... && php bin/console migrate`: still migrated the dev DB `aitessera`. Local php.ini has `variables_order=GPCS`, so `$_ENV` lacks process env; phpdotenv (immutable) then fills `$_ENV` from `.env`, and `Config`/`phinx.php` read only `$_ENV`.
- `php -d variables_order=EGPCS bin/console migrate`: still hit the dev DB, because `bin/console` spawns phinx as a child PHP process without the `-d` flag.

## Watch out
- `php -S ... -t public` changes CWD to `public/`, so relative `JWT_PRIVATE_KEY_PATH=var/keys/...` fails at boot (500 on every request) — pass absolute key paths.
- The Rust port resolves relative key paths against the `.env` directory instead, so the two differ here.

In the code:
- Migrate a test DB: `php -d variables_order=EGPCS vendor/bin/phinx migrate -c phinx.php` (same for `seed:run`)
- Serve: `php -d variables_order=EGPCS -S 127.0.0.1:18305 -t public`
- Config reads: `src/Support/Config.php` (`$_ENV` only), `phinx.php`
- `composer check` spawns phpunit as a subprocess without `-d variables_order=EGPCS`, so env-var DB overrides are lost and DB feature tests silently skip. Run `php -d variables_order=EGPCS vendor/bin/phpunit` directly to see them.
- Running the suite with the benchmark env (RATE_LIMIT_AUTH=1000000) makes the rate-limit pipeline test fail (422 instead of 429). Unset the RATE_LIMIT_* overrides for tests.

In the code: src/Repository/UserRepository.php buildListWhere (:search_email/:search_name), src/Service/AuthService.php refresh, src/Service/UserAdminService.php updateUser, rust/src/service.rs refresh/apply_update.
- A user changing their own password via PATCH /me deliberately does NOT revoke sessions yet. Ending only other devices requires issuing a fresh token pair in the /me response, which changes the API contract. This is an open policy decision, and PHP and Rust must change together.

In the code: src/Service/UserAdminService.php updateUser, rust/src/service.rs update_user (revoke_sessions).
