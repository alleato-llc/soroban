---
name: release-doctor
description: Verify releases actually shipped after a merge to main, diagnose salpa/tag failures (the "workflow success but nothing released" trap), and promote changelogs after a release. Use after any merge touching swift/**, rust/**, or spec/**, or when a release looks wrong.
---

# Release doctor — did it actually ship?

Two salpa-driven tracks, auto-tagged on path-gated merges to `main`:
`swift/**`·`spec/**` → macOS `vX.Y.Z` (`release.yml`, signed DMG);
`rust/**`·`spec/**` → `rust-vX.Y.Z` (`release-rust.yml`). A `spec/**` change
fires BOTH. `site/**` (and `docs/ANZAN.md`, `spec/**`) fires only
`deploy-site.yml`.

Branch protection on `main` requires six checks ("Engine tests", "App
compiles + session tests", "cargo test (ubuntu-latest)", "cargo test
(macos-14)", "npm test + spec (ubuntu-latest)", "playwright (ubuntu-latest)")
and the consolidated `ci.yml` (paths-filter inside one always-running
workflow) makes them always report — skipped-when-irrelevant counts as
passing, so no more admin bypass on merges.

## 1. After a merge: verify, don't trust

```sh
gh run list --branch main --limit 8      # did the right workflows fire?
gh release list --limit 5                # ← the REAL check: is the NEW tag here?
```

**A green Release run does NOT mean a release shipped.** salpa can build,
notarize, and upload the DMG as a workflow *artifact*, hit "a release with the
same tag name already exists", and the job still reports success. If no new
tag appeared, grep the publish job:

```sh
gh run view <run-id> --log --job <publish-job-id> \
  | grep -iE 'salpa version|github release|already exists'
```

## 2. The duplicate-tag disease (diagnosed 2026-07-22)

salpa (`version: git` in `swift/salpa.yaml`) derives the next version from the
tag walk. If two tags point at ONE commit (it happened twice: `v1.4.9`/`v1.4.10`
and `v1.4.11`/`v1.4.12`, from the July 6 history rewrite), the walk picks the
older name → next version == an EXISTING tag → "already exists" soft-fail,
forever, until fixed.

Diagnose: `git tag --points-at $(git rev-list -1 <suspect-tag>)` — two names on
one commit plus `git cat-file -t <tag>` saying `commit` (lightweight) is the
disease. Fix (get user approval — it force-pushes a public tag): re-create the
NEWEST of the pair as an ANNOTATED tag at the SAME commit (annotated wins the
walk; the GitHub release stays linked):

```sh
git tag -f -a vX.Y.Z -m "Soroban X.Y.Z" <same-sha> && git push -f origin vX.Y.Z
git describe --tags --match 'v*' --abbrev=0 main    # must now name the newest
gh run rerun <release-run-id>                        # ship the missed release
```

Notes: salpa attaches only the versionless asset (`Soroban.dmg`); the Rust
track's release also uploads fixed-name assets (`Soroban-cross.dmg`,
`soroban-<os>-<arch>`); `#minor`/`#major` in the merge commit bumps that part.

## 3. After the release ships: promote the changelogs

Promotion is manual, batched, and MUST be a **`[skip ci]`** commit (it touches
release paths but must not cut a release). Move entries from `[Unreleased]`
into dated sections — only entries whose attribution is CERTAIN; leave the
rest visibly unpromoted rather than guessing:

- `swift/CHANGELOG.md`: `## [1.4.13] — 2026-07-22` (em dash), plus the
  compare-links footer (`[1.4.13]: …/compare/v1.4.12...v1.4.13`, and bump the
  `[Unreleased]` compare base).
- `rust/CHANGELOG.md`: `## [rust-v0.1.9] - 2026-07-22` (hyphen, no footer).
- Root `CHANGELOG.md` (cross-cutting): `## [vX.Y.Z] · [rust-vX.Y.Z] — date`
  with release-tag links.

Known backlog: entries spanning the July 5–6 releases can't be attributed
per-version (the duplicate tags above) and stay in `[Unreleased]` on purpose.
