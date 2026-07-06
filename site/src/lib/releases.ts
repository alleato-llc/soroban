// Build-time resolution of the two release tracks' download URLs.
//
// GitHub exposes only ONE repo-wide "latest" release, so
// `releases/latest/download/…` resolves to whichever track (`v*` or `rust-v*`)
// published most recently — it is NOT a reliable per-track link. Instead we ask
// the Releases API for the newest tag of EACH track and read the stable,
// version-free asset names off it (`Soroban.dmg`, `Soroban-cross.dmg`,
// `soroban-<os>-<arch>[.exe]` — see release-rust.yml / docs/RELEASING.md).
//
// Runs in Astro frontmatter, i.e. at BUILD time (Node). It makes a single HTTP
// request; a `release: published` trigger on deploy-site.yml re-runs the build
// so the resolved URLs stay fresh. On ANY failure (offline local build, rate
// limit, missing asset) it falls back to a GitHub URL that always exists, so the
// site build can never break on this.

const REPO = "alleato-llc/soroban";
const API = `https://api.github.com/repos/${REPO}/releases`;
const RELEASES_PAGE = `https://github.com/${REPO}/releases`;

export interface DownloadUrls {
  /** Native macOS build (Swift/AppKit) — the `v*` track's `Soroban.dmg`. */
  swiftDmg: string;
  /** Cross-platform macOS build (Rust/iced) — the `rust-v*` universal dmg. */
  crossDmg: string;
  linuxX64: string;
  linuxArm64: string;
  windowsX64: string;
  windowsArm64: string;
  /** Catch-all: the Releases page, used as the ultimate fallback. */
  releasesPage: string;
}

interface Asset {
  name: string;
  browser_download_url: string;
}
interface Release {
  tag_name: string;
  html_url: string;
  published_at: string;
  draft: boolean;
  assets: Asset[];
}

async function fetchReleases(): Promise<Release[]> {
  const headers: Record<string, string> = {
    Accept: "application/vnd.github+json",
    "User-Agent": "soroban-site-build",
  };
  // A token (present in CI) lifts the unauthenticated 60/hr rate limit.
  const token = process.env.GITHUB_TOKEN;
  if (token) headers.Authorization = `Bearer ${token}`;
  const res = await fetch(API, { headers });
  if (!res.ok) throw new Error(`GitHub Releases API ${res.status}`);
  return (await res.json()) as Release[];
}

/** Newest non-draft release whose tag matches the track predicate. */
function newest(releases: Release[], match: (tag: string) => boolean): Release | undefined {
  return releases
    .filter((r) => !r.draft && match(r.tag_name))
    .sort((a, b) => Date.parse(b.published_at) - Date.parse(a.published_at))[0];
}

/**
 * First asset URL matching any candidate (exact name or regex), in order —
 * else the release's own page, else the list. We prefer the stable, version-free
 * name (`Soroban-cross.dmg`, `soroban-<os>-<arch>`) but fall back to whatever the
 * release actually carries: salpa also publishes the universal dmg as
 * `Soroban-<ver>.dmg` and the portables as `soroban-gui-<os>-<arch>`, so the
 * pattern resolves those too (and keeps working on releases cut before the
 * stable-name step landed).
 */
function pick(rel: Release | undefined, ...candidates: (string | RegExp)[]): string {
  for (const c of candidates) {
    const hit = rel?.assets.find((a) => (typeof c === "string" ? a.name === c : c.test(a.name)));
    if (hit) return hit.browser_download_url;
  }
  return rel?.html_url ?? RELEASES_PAGE;
}

export async function resolveDownloads(): Promise<DownloadUrls> {
  try {
    const releases = await fetchReleases();
    // `v*` but not `rust-v*` is the native track; `rust-v*` is cross-platform.
    const swift = newest(releases, (t) => /^v\d/.test(t));
    const rust = newest(releases, (t) => /^rust-v\d/.test(t));
    return {
      swiftDmg: pick(swift, "Soroban.dmg", /\.dmg$/i),
      // The rust release carries exactly one dmg (the universal build).
      crossDmg: pick(rust, "Soroban-cross.dmg", /\.dmg$/i),
      linuxX64: pick(rust, "soroban-linux-x86_64", /^soroban(-gui)?-linux-x86_64$/i),
      linuxArm64: pick(rust, "soroban-linux-arm64", /^soroban(-gui)?-linux-arm64$/i),
      windowsX64: pick(rust, "soroban-windows-x86_64.exe", /^soroban(-gui)?-windows-x86_64\.exe$/i),
      windowsArm64: pick(rust, "soroban-windows-arm64.exe", /^soroban(-gui)?-windows-arm64\.exe$/i),
      releasesPage: RELEASES_PAGE,
    };
  } catch (err) {
    // Never fail the build on a download-link lookup — every URL degrades to
    // the Releases page, which always resolves.
    console.warn(`[releases] using Releases-page fallback: ${err}`);
    return {
      swiftDmg: RELEASES_PAGE,
      crossDmg: RELEASES_PAGE,
      linuxX64: RELEASES_PAGE,
      linuxArm64: RELEASES_PAGE,
      windowsX64: RELEASES_PAGE,
      windowsArm64: RELEASES_PAGE,
      releasesPage: RELEASES_PAGE,
    };
  }
}
