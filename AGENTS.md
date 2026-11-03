# Repository workflow

Use Git Flow as described in [CONTRIBUTING.md](CONTRIBUTING.md).

- Start normal work from `develop` on a `feature/<name>` branch.
- Use `release/<version>` for release preparation and `hotfix/<name>` from `main` for urgent release fixes.
- Use these Git Flow branch names instead of the default `codex/` prefix.
- Keep `main` for releases; merge releases and hotfixes back into `develop`.
- Run `./scripts/check.sh` for code changes before merging.
