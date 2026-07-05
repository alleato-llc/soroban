# Releasing Soroban

Releases are **automatic and split by ecosystem into two independent tracks**.
Work happens on branches and PRs (CI runs tests); a push/merge to `main` runs
the release track(s) whose paths it touched. Each track can also be run by hand
from the Actions tab (**Run workflow** → `workflow_dispatch`). A `spec/**` change
is shared language behavior and releases **both** tracks.

## macOS track — `release.yml` (salpa)

Triggered by `swift/**` or `spec/**` changes. Driven by
[salpa](https://github.com/alleato-llc/salpa) (our house release tool, pulled
from ghcr as an OCI artifact):

1. runs the engine + session test suites as two parallel jobs (`salpa test
   engine` / `salpa test session` — the suites are defined in `swift/salpa.yaml`),
2. **auto-tags the next semantic version** (`salpa version`) — patch by
   default; put `#minor` or `#major` anywhere in the merge/head commit message
   to bump that part instead (a HEAD already tagged `v*` releases that tag as-is),
3. `salpa build` imports your Developer ID cert into an ephemeral keychain,
   builds the app **signed** (the tag's version becomes `MARKETING_VERSION`),
   packages a `Soroban-X.Y.Z.dmg`, **notarizes** it with Apple, staples the ticket,
4. `salpa publish` cuts the GitHub Release `vX.Y.Z`, attaching the versionless
   `Soroban.dmg` — the stable latest-release download the site's button points at.

## Rust track — `release-rust.yml`

Triggered by `rust/**` or `spec/**` changes. The Rust/iced app (`rust/gui`) is
cross-platform, so this ships **portable, unsigned** binaries for Linux, Windows,
and macOS — the signed native macOS app is the Swift DMG above.

1. a **test gate** runs the Rust unit tests + the shared Gherkin suite,
2. **its own version sequence** — `rust-vX.Y.Z` tags, computed in-workflow by
   bumping the latest `rust-v*` tag (patch default; `#minor`/`#major` in the head
   commit bumps bigger; a HEAD already tagged `rust-v*` re-releases it). The
   `v*` and `rust-v*` namespaces are independent and never collide,
3. a **6-target matrix** (Linux / Windows / macOS × x86_64 / arm64; macOS
   arm64 native + x86_64 cross-compiled on Apple Silicon) builds `rust/gui`,
4. each binary attaches to the `rust-vX.Y.Z` GitHub Release under a versionless
   name (`soroban-gui-<target>[.exe]`, `--clobber`).

The Rust track needs **no secrets** (binaries are unsigned; the `rime` sibling
repo is public). It checks out `soroban` and `rime` as siblings so `rust/gui`'s
`../../../rime/rime` path dependency resolves.

## Common notes

The **GitHub Release is the point of truth** for downloads; neither release
workflow touches any cloud. Because GitHub exposes one repo-wide "latest"
release, `releases/latest/download/...` resolves to whichever track released most
recently — per-track stable links are a landing-page concern (deferred).

**Changelogs are split per ecosystem** — `swift/CHANGELOG.md` (dated `v*`),
`rust/CHANGELOG.md` (`rust-v*`), and the repo-root `CHANGELOG.md` for
cross-cutting changes. salpa (and the Rust track) **never edit a changelog** —
write each change under the right `## [Unreleased]` in the same commit; a
maintainer promotes it to a dated section later. A commit that should NOT cut a
release — docs-only, a CHANGELOG promotion, or test-only touching a release path
— must carry `[skip ci]` (it suppresses all release workflows; path-gating
already spares pushes that don't touch a track's paths).

A separate workflow, `deploy-site.yml`, publishes the landing page + living
spec/report to the site host (`soroban.alleato.dev`) on `site/**`,
`docs/ANZAN.md`, `spec/**`, or `swift/Engine/**` changes — `salpa deploy`. Its
deploy credentials are repository variables/secrets (see the workflow); the
hosting infra is provisioned out of band, separate from this repo.

## One-time setup: the five secrets

In the GitHub repo: **Settings → Secrets and variables → Actions → New
repository secret**.

| Secret | Value |
|---|---|
| `BUILD_CERTIFICATE_BASE64` | your Developer ID Application certificate **with its private key**, exported as `.p12`, base64-encoded |
| `P12_PASSWORD` | the password you chose during the `.p12` export |
| `APPLE_TEAM_ID` | the 10-character team id (developer.apple.com → Membership) |
| `APPLE_ID` | the Apple ID email used for notarization |
| `APPLE_APP_SPECIFIC_PASSWORD` | an app-specific password — create at [appleid.apple.com](https://appleid.apple.com) → Sign-In and Security → App-Specific Passwords |

### Pulling salpa

Both workflows pull the `salpa` binary from ghcr
(`ghcr.io/alleato-llc/salpa`) via `oras`, authenticated with the workflow's
`GITHUB_TOKEN` (`packages: read`). salpa is a **private** package; the repo is
granted read access under the package's *Manage Actions access* settings, so
no PAT is needed.

### Exporting the certificate

You need a **Developer ID Application** certificate (not "Apple
Development" / "Mac App Distribution"). If you don't have one yet: Xcode →
Settings → Accounts → Manage Certificates → + → Developer ID Application
(or developer.apple.com → Certificates).

1. Open **Keychain Access** → My Certificates.
2. Find "Developer ID Application: Your Name (TEAMID)" — expand it and
   confirm the private key is underneath (no key = export from the Mac that
   created the certificate).
3. Right-click the certificate → **Export…** → format `.p12`, choose a
   password (that's `P12_PASSWORD`).
4. Base64 it onto the clipboard and paste into the secret:

   ```sh
   base64 -i Certificates.p12 | pbcopy
   ```

## Day-to-day

```sh
git checkout -b feature/thing     # CI runs tests on every push
…                                 # open a PR, merge to main
                                  # swift/** or spec/** → macOS release  (v1.0.X, the dmg)
                                  # rust/**  or spec/** → Rust release   (rust-v0.1.X, the binaries)
```

- Bigger bumps: include `#minor` or `#major` in the merge commit message (applies
  to whichever track the commit's paths trigger).
- A failed release (e.g. before the secrets existed, or a notarization
  hiccup): fix the cause and **re-run the workflow run** — the HEAD is
  already tagged, so it rebuilds the same version instead of bumping again.
- Verify a downloaded dmg locally:

  ```sh
  spctl -a -t open --context context:primary-signature -v Soroban-1.0.0.dmg
  xcrun stapler validate Soroban-1.0.0.dmg
  ```

## Keeping the CHANGELOGs in sync

Neither release track edits a changelog — they only tag. **Changelogs are split
per ecosystem:**

- `swift/CHANGELOG.md` — the macOS app / engine / CLI (dated `## [vX.Y.Z]`).
- `rust/CHANGELOG.md` — the Rust crates + `rust/gui` (dated `## [rust-vX.Y.Z]`).
- repo-root `CHANGELOG.md` — **cross-cutting** changes only (shared `spec/**`,
  monorepo layout, cross-ecosystem interchange, common CI/release infra).

So:

- Write each change under `## [Unreleased]` in the **right** file, in the **same
  commit** as the code. A `spec/**` change is shared — note it in the root file
  *and* in each ecosystem's changelog.
- When a version ships (or the file drifts), **promote** those notes into a dated
  section (`## [vX.Y.Z]` / `## [rust-vX.Y.Z]`) with a compare-link footer. A
  one-version lag is normal — don't chase your own tail.
- A commit that should **not** cut a release — docs-only, a CHANGELOG promotion,
  or test-only touching a release path — must carry **`[skip ci]`**. Path-gating
  already spares pushes that don't touch a track's paths (`swift/**`, `rust/**`,
  `spec/**`); `[skip ci]` is the override when a path-matching commit still
  shouldn't release.

## Notes

- The runner builds with Xcode 26.2 on `macos-26` (PickleKit's Gherkin
  suite needs Swift 6.2+); the app targets macOS 14+.
- Local builds stay ad-hoc signed (`CODE_SIGN_IDENTITY: "-"` in
  project.yml); salpa overrides signing on the `xcodebuild` command line (and
  imports the cert into a throwaway keychain on the runner), so nothing
  changes for development. `salpa build --explain` prints the exact commands.
- Hardened runtime (a notarization requirement) is already on in
  project.yml.
