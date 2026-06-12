# Releasing Soroban

Releases are **gitflow-style and automatic**, driven by
[salpa](https://github.com/alleato-llc/salpa) (our house release tool, pulled
from ghcr as an OCI artifact). Work happens on branches and PRs (CI runs tests
+ an unsigned compile); every push/merge to `main` runs the Release workflow,
which

1. runs the engine + session test suites as two parallel jobs (`salpa test
   engine` / `salpa test session` — the suites are defined in salpa.yaml),
2. **auto-tags the next semantic version** (`salpa version`) — patch by
   default; put `#minor` or `#major` anywhere in the merge/head commit message
   to bump that part instead (a HEAD that is already tagged `v*` releases that
   tag as-is),
3. `salpa build` imports your Developer ID cert into an ephemeral keychain,
   builds the app **signed** (the tag's version becomes `MARKETING_VERSION`),
   packages a `Soroban-X.Y.Z.dmg` (with an /Applications drop link), signs it,
   **notarizes** it with Apple, staples the ticket,
4. `salpa publish` cuts the GitHub Release `vX.Y.Z`, attaching the
   `Soroban-X.Y.Z.dmg` **and** a versionless `Soroban.dmg` — the stable
   latest-release download the site's button points at.

The **GitHub Release is the point of truth** for downloads; the release
workflow touches no cloud. A separate workflow, `deploy-site.yml`, publishes the
landing page + living spec/report to the site host (`soroban.alleato.dev`) on
`site/**`, `docs/ANZAN.md`, or `Engine/**` changes — `salpa deploy`. Its
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
                                  # → Release tags v1.0.X and ships the dmg
```

- Bigger bumps: include `#minor` or `#major` in the merge commit message.
- A failed release (e.g. before the secrets existed, or a notarization
  hiccup): fix the cause and **re-run the workflow run** — the HEAD is
  already tagged, so it rebuilds the same version instead of bumping again.
- Verify a downloaded dmg locally:

  ```sh
  spctl -a -t open --context context:primary-signature -v Soroban-1.0.0.dmg
  xcrun stapler validate Soroban-1.0.0.dmg
  ```

## Notes

- The runner builds with Xcode 26.2 on `macos-26` (PickleKit's Gherkin
  suite needs Swift 6.2+); the app targets macOS 14+.
- Local builds stay ad-hoc signed (`CODE_SIGN_IDENTITY: "-"` in
  project.yml); salpa overrides signing on the `xcodebuild` command line (and
  imports the cert into a throwaway keychain on the runner), so nothing
  changes for development. `salpa build --explain` prints the exact commands.
- Hardened runtime (a notarization requirement) is already on in
  project.yml.
