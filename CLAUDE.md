# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

AITessera — Secure JWT-based authentication & authorization API for the AIvance service suite.
A single REST API project built on **pure, framework-less modern PHP (8.3+)**. It is assembled by
combining vetted standard libraries, and security-critical pieces such as JWT and encryption are
never implemented directly — they are always delegated to trusted libraries.

> **Common rules are automatically inherited from the global [`~/.claude/CLAUDE.md`](~/.claude/CLAUDE.md)** (language, Git workflow, security, code style, testing, API design, load distribution, log pipeline, PHP LSP). This document defines only **AITessera repository-specific** rules (framework-less stack, architecture, CI/CD).
>
> Since this is a framework-less project, CI4-specific terms from the global rules map as follows: `esc()` → `htmlspecialchars`/JSON, Query Builder → PDO prepared statements, `model()` → DI injection, `.env` → `vlucas/phpdotenv`.

## Tech Stack

Instead of a framework, this combines lightweight, PSR-standard-based libraries.

| Area | Choice | Notes |
|------|------|------|
| **Language** | PHP 8.4+ | `readonly`, enums, full type declarations |
| **Dependency management** | Composer | PSR-4 autoloading |
| **HTTP messages** | PSR-7 (`nyholm/psr7`) | framework-independent |
| **Middleware dispatcher** | PSR-15 (`relay/relay`) | auth, CORS, error-handling pipeline |
| **Routing** | `nikic/fast-route` | ultra-lightweight router |
| **DI container** | `php-di/php-di` (PSR-11) | constructor injection |
| **JWT** | `lcobucci/jwt` | never implement directly · pin the `alg` |
| **DB access** | PDO (prepared statements) | never assemble raw strings |
| **Migrations** | `robmorgan/phinx` | defined via `phinx.php` |
| **Env vars** | `vlucas/phpdotenv` | loads `.env` |
| **Input validation** | `respect/validation` | enforced at controller entry |
| **Rate limiting** | `symfony/rate-limiter` | brute-force defense |
| **Cache/queue** | Redis (`predis/predis`) | shared for cache and async queue |
| **API docs** | `zircote/swagger-php` (v5) | OpenAPI attributes → **RapiDoc** UI (`/api/docs`, API list on the left) |
| **Static analysis** | PHPStan (level 8) | across all of `src/` |
| **Code style** | PHP-CS-Fixer (PSR-12) | auto-formats before commit |
| **Testing** | PHPUnit | unit and integration |

> For auth design principles (Access/Refresh token separation, rotation, fixed algorithm, etc.), see the global [`~/.claude/rules/security.md`](~/.claude/rules/security.md).

## Local Environment Setup

```bash
cp .env.example .env      # copy .env, then set the required keys below
composer install
php bin/console migrate   # run migrations (phinx wrapper)
```

Required `.env` keys:

```env
# App
APP_ENV = local
APP_BASE_URL = http://localhost:8305/

# DB
DB_HOST = localhost
DB_NAME = aitessera
DB_USER = aitessera
DB_PASS =

# Redis
REDIS_HOST = 127.0.0.1
REDIS_PORT = 6379

# JWT (required — a random string of 32+ characters, for HS256)
JWT_SECRET = E39fksd4fsl90Adfk27fjdkAedkdfh
JWT_ACCESS_TTL  = 900       # in seconds (15 minutes)
JWT_REFRESH_TTL = 1209600   # in seconds (14 days)
```

> Secrets must **never be hardcoded in code or the repository**. In production, use AWS SSM Parameter
> Store / Secrets Manager, and never commit `.env`.

## Commands

```bash
composer serve                 # dev server — PHP built-in server (php -S 127.0.0.1:8305 -t public, tessera.test)
composer test                 # run PHPUnit alone
composer analyse              # run PHPStan alone
composer cs-fix               # auto-format with PHP-CS-Fixer
composer check                # cs-check + PHPStan + PHPUnit in sequence
php bin/console <command>      # CLI entry point (migrations, queue consumers, etc.)
```

## Directory Conventions

Since there is no framework, the structure is defined directly. Only the **front controller
(`public/index.php`)** is exposed to the web; everything else lives under `src/`.

| Path | Purpose |
|------|------|
| `public/index.php` | the sole entry point (front controller) · web server DocumentRoot |
| `src/Controller/` | HTTP request handling — keep thin |
| `src/Service/` | business logic · transaction management |
| `src/Repository/` | PDO-based data access layer |
| `src/Middleware/` | PSR-15 middleware (`JwtAuthMiddleware`, etc.) |
| `src/Domain/` | DTOs (`readonly`) · enums · domain models |
| `src/Exception/` | domain exceptions |
| `src/Support/` | shared utilities (JWT, DB connection, queue, mail, response, etc.) |
| `src/Console/` | CLI workers such as queue consumers (`ProcessMailQueue`, `ProcessLogQueue`) |
| `src/OpenApi/Schema/` | reusable OpenAPI schema components (docs-only classes) |
| `config/` | container definitions · route definitions · config loading |
| `migrations/` | phinx migrations |
| `bin/console` | CLI entry point (`migrate`, `seed:run`, `mail:work`, `log:work`) |
| `var/` | runtime output (logs, cache) — not tracked by git |
| `docs/` | project documentation |
| `legacy-source/` | legacy source kept for reference (do not add to git) |

## Core Architecture Patterns

### Request handling flow

```
public/index.php
  → load .env → build DI container
  → PSR-15 middleware pipeline (Relay)
      [ErrorHandler → TrailingSlash → Cors → RateLimit → JwtAuth → RouteDispatch]
  → Controller → Service → Repository (PDO)
  → return PSR-7 Response
```

> `TrailingSlashMiddleware` normalizes trailing slashes (`/api/docs/` → `/api/docs`). Since public
> paths and routing use exact matching, omitting this normalization causes 401/404 on a trailing slash.

### JWT authentication flow

`JwtAuthMiddleware` (PSR-15) validates the `Authorization: Bearer` token via `lcobucci/jwt`, then
carries the user ID in the request context to the next middleware. Controllers pull it out of a
request attribute. It is valid **only within the request scope**, with no separate global state.

```php
// JwtAuthMiddleware → inject into a request attribute after validation
$request = $request->withAttribute('userId', (int) $claims->get('sub'));
return $handler->handle($request);

// used in a Controller
$userId = (int) $request->getAttribute('userId');
```

> JWT validation constraints (`SignedWith`, `StrictValidAt`, fixed algorithm) are key to preventing
> auth bypass — see the global [`~/.claude/rules/security.md`](~/.claude/rules/security.md).

### Absolute prohibitions for a framework-less architecture

In addition to the layer responsibilities in the global [`~/.claude/rules/code-style.md`](~/.claude/rules/code-style.md), the following are especially enforced since there is no framework.

| Prohibited | Reason |
|------|------|
| Running SQL directly in a Controller or Service | data access must be isolated in the Repository |
| Referencing `PDO`/`Redis` directly as a global/singleton | must go through the DI container (constructor injection) |
| Instantiating `new Service()` directly | the container injects it — manual `new` is prohibited |
| Manipulating auth state outside middleware | auth is handled in one place only: `JwtAuthMiddleware` |
| Exposing files outside `public/` to the web | the only entry point is `public/index.php` |

### OpenAPI docs (where they're organized)

Follow the OpenAPI rules in the global [`~/.claude/rules/api-design.md`](~/.claude/rules/api-design.md); AITessera specifics:
- Define **request/response schemas** as `#[OA\Schema]` components under `src/OpenApi/Schema/` and reference them with `$ref` (no inline duplication).
- Every 4xx/5xx response references the shared `ErrorResponse` component.
- Global definitions (Info, Server, the `bearerAuth` security scheme) live in `src/OpenApiSpec.php`.
- Spec path: `/api/v1/openapi.json` (swagger-php v5 scans `src/` → JSON; cached in production).

## Verification Gates — What Runs Where

Verification is completed locally. CI is not run on `feature/*` → `dev` PRs; it runs only on
`dev` → `main` deploy PRs.

```
feature/*  ──[local verification: pre-push hook]──▶  dev  ──[PR + CI]──▶  main
                    ↑                        ↑
              the real gate is here      CI only runs here
```

| When | What | Who |
|------|--------|------|
| During development | run `composer analyse` + `composer test` as needed | person / Claude |
| Before pushing to `dev` | `composer check` (cs-check + PHPStan + PHPUnit) is required — do not push if it fails | pre-push hook |
| `feature/*` → `dev` PR | No CI. Code review only | — |
| `dev` → `main` PR | Full GitHub Actions (cs-check + PHPStan + migrate + PHPUnit) | CI |

Having no CI on `feature → dev` means the `dev` branch can receive unverified code.
If several features pile up in that state and CI runs for the first time on the deploy PR, it
becomes expensive to find which commit broke things, and deployment gets blocked. **Local
verification is the only line of defense, so skipping it is a rule violation.** The same applies
when Claude does the work — actually run `composer check` and check the output before creating a
PR targeting `dev`. Don't move on assuming "it'll probably pass."

### Enforced via Git Hooks

Rather than relying on habit, this is enforced via committed hooks. Activate once right after
cloning (usually no manual setup is needed, since `composer install`'s `post-install-cmd` runs it
automatically):

```bash
git config core.hooksPath .githooks
```

| Hook | Behavior |
|----|------|
| `pre-commit` | Auto-formats staged `*.php` with PHP-CS-Fixer and re-stages it. Does not block the commit |
| `pre-push` | Runs `composer check` only when the push target is `dev`; aborts the push on failure |
| `pre-push` | Unconditionally blocks direct pushes to `main` — deployment only via a `dev` → `main` PR (merge commit) |

- Pushes to `feature/*` are not verified — to avoid slowing down fast iteration during development.
- Docs-only changes (pushes that only touch `*.md`, `docs/**`, `.claude/rules/**`) are automatically
  skipped by `pre-push` since there's no code to compare against, even when targeting `dev`. As soon
  as even a single line of code is mixed in, full verification kicks back in immediately.
- Emergency bypass: `SKIP_HOOKS=1 git push ...` (or `git commit ...`)
- With partial staging via `git add -p`, `pre-commit` may pull unstaged changes into the commit too —
  use `SKIP_HOOKS=1` in that case.
- In an environment without PHP/Composer, both hooks automatically skip verification and CI performs
  the final check (however, `pre-push`'s block on direct pushes to `main` always applies regardless
  of whether Composer is present).

## CI (GitHub Actions)

Automatic verification runs only on `dev` → `main` **deploy PRs** (not on `feature/*` → `dev` PRs).
Defined in: `.github/workflows/ci.yml`.

```yaml
on:
  pull_request:
    branches: [main]     # does not run on PRs targeting dev
```

Leaving `branches` empty would run this on every PR, defeating the policy above. Since `dev` → `main`
deploy PRs are merged with a merge commit (global rule), the exact commit combination that passed CI
lands on `main` as-is.

- **Concurrency**: in-progress runs are canceled when a new push lands on the same ref (`concurrency.cancel-in-progress`)

### Runs on a self-hosted runner

Instead of a GitHub-hosted runner, this runs on **a single organization (`aivance-kr`) self-hosted
runner** (triggered by a hosted-runner billing issue). Both CI (the `backend` job) and CD (the
`deploy` job) use the same runner — `runs-on: [self-hosted, Linux, X64]`.

- **MySQL/Redis**: Linux self-hosted runners natively support `services:` Docker containers.
  The `backend` job declares MySQL and Redis via a `services:` block — the Actions runner handles
  startup, health checks, and cleanup after the job ends, so no separate `docker run`/`docker rm`
  steps are needed.
- **Ports**: this runner uses host ports that don't overlap with other repositories' self-hosted CI
  service containers — MySQL `23306`, Redis `26379` (overridable via `CI_MYSQL_PORT`/`CI_REDIS_PORT`).
  When adding a new repository to this runner, always check that ports don't collide.
- **Reverting to a hosted runner**: change `runs-on` back to `ubuntu-latest` — the `services:` block
  works the same way on GitHub-hosted Linux runners.
- **Claude does not register runners on your behalf** — since this is a system-level configuration
  change (installing a system service, etc.), a person must run the commands shown on the GitHub
  organization's `Settings → Actions → Runners → New runner` page directly.

### `backend` job — PHP · PHP-CS-Fixer · PHPStan · PHPUnit

With MySQL and Redis started as `services:` containers, verification runs in this order:

1. setup-php `8.4` (extensions: `mbstring intl pdo_mysql redis curl dom xml tokenizer`, coverage via `pcov`)
2. Composer cache → `composer install`
3. Copy `.env.example` → `.env`, then inject the CI DB, Redis, and `JWT_SECRET`
4. Create subdirectories under `var/` (not tracked by git, to guarantee the runtime path)
5. `composer cs-check` (PHP-CS-Fixer dry-run)
6. `composer analyse` (PHPStan level 8)
7. Build the test schema via `php bin/console migrate`
8. `composer test` (PHPUnit unit and DB integration)

> New PHP code must pass PHPStan level 8 and have green related PHPUnit tests to pass CI. Write `tests/` tests alongside every new feature.

## CD (Deployment)

A push to `main` (i.e., a `dev → main` PR merge) triggers **automatic SSH deployment** to the
production server. Defined in: `.github/workflows/deploy.yml`.

- **Trigger**: push to `main` + `workflow_dispatch` (manual / rollback)
- **Concurrency**: `deploy-production` group — only 1 concurrent deployment, `cancel-in-progress: false`
- **Job runner**: runs on the same organization self-hosted runner as CI (`runs-on: [self-hosted, Linux, X64]`)
- **Target**: a single Ubuntu + mod_php (or PHP-FPM) Apache server (deployed via SSH using appleboy/ssh-action — the deploy target server is separate from this job runner)

### Deployment procedure (executed via SSH on the server)

1. `git reset --hard origin/main` — bring in the latest main
2. Create the `var/` directory — **must happen before composer/migrate** (to guarantee runtime log/cache paths)
3. `composer install --no-dev --optimize-autoloader`
4. `php bin/console migrate` — grep the output for exceptions and `exit 1` to abort the deploy if found
5. Clear caches (OPcache / Redis application cache)
6. `sudo -n systemctl reload apache2` (or `php-fpm`) — refresh OPcache (zero downtime)
7. **Slack notification** — after the SSH deploy steps (1-6) finish, the GitHub Actions runner sends
   the result to an Incoming Webhook (`SLACK_WEBHOOK_URL`) in separate success (`if: success()`) and
   failure (`if: failure()`) steps

> **Migration pitfall**: a DB connection failure or migration exception can still leave the CLI exit
> code at 0. `set -e` may not catch this, so capture the output, check it directly for exception
> patterns (`[...Exception]`, `Unable to connect`, `Access denied`), and abort the deploy on failure.

> **var/ chmod pitfall**: `var/cache`/`var/logs` files created at runtime by Apache (`www-data`) are
> not owned by the deploy account, so `chmod -R 775 var` fails with `Operation not permitted`.
> Treat this `chmod` as best-effort (`2>/dev/null || echo …`); the root fix is the setgid setup in
> server prep below.

### Required GitHub Secrets (`production` environment)

`DEPLOY_HOST` · `DEPLOY_USER` · `DEPLOY_SSH_KEY` · `DEPLOY_PORT` · `DEPLOY_PATH`

- `SLACK_WEBHOOK_URL` (optional) — when set, notifies deploy success/failure via a Slack Incoming
  Webhook. If unset, the notification step is automatically skipped and the deployment itself is
  unaffected.

### One-time server preparation

- **GitHub read-only deploy key** — set the server repo remote to SSH (`git@github.com:...`) (HTTPS fails with `could not read Username`)
- **Production `.env`** with real DB, Redis, and JWT credentials (missing this causes `Access denied` during migrate)
- **Passwordless sudo**: in `/etc/sudoers.d/aitessera-deploy`, `<DEPLOY_USER> ALL=(ALL) NOPASSWD: /usr/bin/systemctl reload apache2` (missing this fails with `sudo: a password is required`)
- Apache's `DocumentRoot` is `public/`; `var/` must be writable by the Apache user (`www-data`)
- **var/ setgid setup (recommended)** — eliminates chmod failures caused by ownership conflicts at the root:
  ```bash
  sudo chown -R <DEPLOY_USER>:www-data <DEPLOY_PATH>/var
  sudo chmod -R 2775 <DEPLOY_PATH>/var   # setgid: new files inherit the www-data group
  ```

### After deployment — default admin account (one-time)

Deployment includes only migrations; **seeders are not run automatically.** If there is no admin account, run this once on the server (safe to re-run).

```bash
cd <DEPLOY_PATH> && php bin/console seed:admin
```

## Cloud/Infrastructure (Reference)

- **Default AWS stack**: ECS (Fargate) + RDS + ElastiCache (Redis) + SQS
- **Secrets management**: never commit `.env` — use AWS SSM Parameter Store / Secrets Manager
- **Logs**: aim for structured (JSON) logging
- **Health check**: recommended to provide a `GET /health` endpoint (including DB/cache connection status)
