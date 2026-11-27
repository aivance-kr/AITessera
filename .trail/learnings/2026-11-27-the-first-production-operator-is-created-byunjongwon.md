---
title: "The first production operator is created with a console command, never with the dev seeder"
author: Byunjongwon
date: 2026-11-27
branch: feature/operator-create-command
cost: { minutes: 5, tokens: 72068 }
id: l-f97f0ea8
scope: repo
status: active
src: agent
touches:
  - AGENTS.md
  - CLAUDE.md
  - README.md
  - rust/README.md
  - bin/console
  - src/Console/CreateOperator.php
  - src/Service/UserService.php
  - tests/Unit/CreateOperatorTest.php
  - tests/Unit/UserServiceTest.php
  - migrations/seeds/TestUserSeeder.php
---

## Decided
- Production has no seeder run on deploy, so the first operator (role=1) is created on the server with a console command. It goes through the service with no creator check, but only for the operator role.
- The password is read from a hidden prompt (asked twice) or the first stdin line, never from an argument, so it stays out of shell history and the process list.
- Re-running with an email that already exists in the same affiliation is a no-op that exits 0, so the runbook step is safe to repeat.

## Tried and ruled out
- `seed:admin`: the docs told people to run it after deploy, but it was never implemented. Check `php bin/console list` before trusting a command named in the docs.
- Mocking UserService in the console test: it is `final readonly`, and the repo has no bypass-finals. The test builds a real UserService with mocked repositories instead.

## Watch out
- The dev seeder creates admin@aivance.test with a password committed to the repo. Never run `seed:run` in production.
- The console command builds the full container, so JWT keys must be readable (var/keys) even though no token is issued.

In the code:
- `php bin/console operator:create --email= --name= --affiliation= --contact= [--company=]`
- src/Console/CreateOperator.php, UserService::createInitialOperator, migrations/seeds/TestUserSeeder.php
