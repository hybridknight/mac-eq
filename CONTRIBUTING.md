# Contributing

## Git Flow

- `main` contains released code.
- `develop` integrates work for the next release.
- Create `feature/<name>` branches from `develop` and merge them back into `develop`.
- Create `release/<version>` branches from `develop` for release preparation. Merge a completed release into both `main` and `develop`, and tag the merge on `main` as `v<version>`.
- Create `hotfix/<name>` branches from `main` for urgent release fixes. Merge a completed hotfix into both `main` and `develop`, and tag the new release on `main`.

Use merge commits (`--no-ff`) to retain branch history. Run `./scripts/check.sh` before merging code changes. Publish branches and tags when ready for collaboration or release.

Start a feature with ordinary Git commands:

```sh
git switch develop
git switch -c feature/my-change
```

After completing and reviewing the change:

```sh
git switch develop
git merge --no-ff feature/my-change
git branch -d feature/my-change
```

The optional `git-flow` extension can manage the same workflow. Its local configuration uses `main`, `develop`, the standard branch prefixes, and the `v` version tag prefix. For a fresh clone with the extension installed, run `git flow init` and select those settings; local Git configuration is not transferred by cloning.
