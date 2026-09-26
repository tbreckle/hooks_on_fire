# Development workflow

Hooks on Fire uses the **GitFlow** branching model with GitHub Actions for builds and releases.
The workflows are described in [.github/WORKFLOWS.md](../../.github/WORKFLOWS.md).

## Branches

- **`main`** – released code. Every merge into `main` comes from a release or hotfix PR and is
  tagged `vX.Y.Z`.
- **`develop`** – integration branch, base for all feature work. Should always build.
- **`feature/*`** (e.g. `feature/point-blank`) – from `develop`, PR back into `develop`.
- **`bugfix/*`** (e.g. `bugfix/42-serial-timeout`) – from `develop`, PR back into `develop`.
- **`release/X.Y.Z`** – created by **Release Start** from `develop`, PR into `main`. Only fixes
  for this release.
- **`hotfix/X.Y.Z`** – created by **Release Start** from `main`, PR into `main`. Critical fixes
  for the released version.

`main` and `develop` are protected: changes go through pull requests with passing CI (see
[branch-protection.conf](../../.github/branch-protection.conf)).

## Feature

```bash
git switch develop && git pull
git switch -c feature/my-feature
# … commits, add an entry under [Unreleased] in CHANGELOG.md …
git push -u origin feature/my-feature
```

Then open a pull request to `develop`.

## Release

1. GitHub Actions → **Release Start** → Run workflow, `kind: release`, and the part to bump
   (`major`/`minor`/`patch`).
2. The workflow computes the next version from the latest `vX.Y.Z` tag, creates
   `release/X.Y.Z` from `develop`, moves the `[Unreleased]` CHANGELOG entries into a
   `[X.Y.Z]` section and opens a pull request to `main`.
3. CI builds the release branch as `X.Y.Z-rc.N`. Only fixes for this release go onto it.
4. Merge the PR with **Create a merge commit** (no squash/rebase). **Release Finish** then builds
   `X.Y.Z` from the merge commit, creates the tag `vX.Y.Z` and the GitHub Release (archives for
   Linux, Windows and macOS, `SHA256SUMS`, notes from CHANGELOG.md), opens a back-merge PR into
   `develop` and deletes the release branch.
5. Merge the back-merge PR (again with a merge commit). CHANGELOG entries added to `develop` in
   the meantime stay under `[Unreleased]`.

## Hotfix

1. GitHub Actions → **Release Start** with `kind: hotfix` creates `hotfix/X.Y.Z` (patch bump)
   from `main` and opens a PR to `main`.
2. Push the fix to the hotfix branch and add it to the `[X.Y.Z]` section in CHANGELOG.md.
3. Merge the PR: Release Finish runs as for a release, including the back-merge PR into `develop`.

## Versioning

Versions follow [Semantic Versioning](https://semver.org/) and are computed from the branch and
the tags by `scripts/version.sh`:

| Branch / ref | Version | Meaning |
|---|---|---|
| tag `vX.Y.Z` (on `main`) | `X.Y.Z` | official release |
| `release/X.Y.Z`, `hotfix/X.Y.Z` | `X.Y.Z-rc.N` | release candidate, N = commits since the branch left `main` |
| `develop`, `feature/*`, `bugfix/*`, anything else | `0.0.0+<sha>` | unofficial build |

- **MAJOR** – incompatible changes, e.g. to `hof-config.yaml`, device or game files
- **MINOR** – new features, backward compatible
- **PATCH** – bug fixes

No version is committed: `Cargo.toml` stays at `0.0.0`, CI injects the version at build time
(`HOF_VERSION`). So the back-merge brings no version into `develop`.

```bash
scripts/version.sh                                            # version of the current checkout
scripts/version.sh next minor                                 # next minor release version
HOF_VERSION=$(scripts/version.sh) cargo build --workspace --release   # local build with version
```
