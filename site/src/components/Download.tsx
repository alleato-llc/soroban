import { useEffect, useState } from "preact/hooks";
import type { DownloadUrls } from "../lib/releases";

// Platform-aware download control. The build-time resolver (src/lib/releases.ts)
// passes the resolved per-track URLs in as props; this island only detects the
// visitor's OS/arch client-side and picks which button(s) to show.
//
// Progressive enhancement: the server-rendered (pre-hydration) and JS-off state
// is "unknown" → a full all-platforms list, so it's useful with no JavaScript.
// On macOS the visitor chooses between the NATIVE (Swift) and CROSS-PLATFORM
// (Rust) build; on Windows/Linux they get the matching portable binary.

type OS = "mac" | "windows" | "linux" | "unknown";
type Arch = "x64" | "arm64";

interface NavigatorUAData {
  platform?: string;
  getHighEntropyValues?: (hints: string[]) => Promise<{ architecture?: string }>;
}

function detectOS(): OS {
  if (typeof navigator === "undefined") return "unknown";
  const ua = navigator.userAgent;
  // No desktop build for phones/tablets → fall through to the list.
  if (/android|iphone|ipad|ipod/i.test(ua)) return "unknown";
  const uaData = (navigator as unknown as { userAgentData?: NavigatorUAData }).userAgentData;
  const plat = (uaData?.platform ?? navigator.platform ?? "").toLowerCase();
  if (/mac/.test(plat) || /mac os x/i.test(ua)) return "mac";
  if (/win/.test(plat) || /windows/i.test(ua)) return "windows";
  if (/linux|x11/.test(plat) || /linux/i.test(ua)) return "linux";
  return "unknown";
}

export default function Download(urls: DownloadUrls) {
  const [os, setOS] = useState<OS>("unknown");
  const [arch, setArch] = useState<Arch>("x64");

  useEffect(() => {
    setOS(detectOS());
    // Arch only matters for the Windows/Linux portable binaries (the macOS dmgs
    // are universal). Prefer the high-entropy UA hint; fall back to the UA
    // string; default x64.
    const uaData = (navigator as unknown as { userAgentData?: NavigatorUAData }).userAgentData;
    if (uaData?.getHighEntropyValues) {
      uaData
        .getHighEntropyValues(["architecture"])
        .then((v) => {
          if (v.architecture === "arm") setArch("arm64");
        })
        .catch(() => {});
    } else if (/arm64|aarch64/i.test(navigator.userAgent)) {
      setArch("arm64");
    }
  }, []);

  // Windows/Linux each ship two portable binaries (x86-64 + ARM64). Present BOTH
  // as equal, side-by-side buttons — the visitor's detected arch simply leads
  // (accent) so it's easy to grab, but neither arch is demoted to a fine-print
  // link. Mirrors the macOS native/cross pair below.
  const archButtons = (label: string, x64: string, arm64: string) => {
    const first = arch === "arm64" ? { href: arm64, name: "ARM64" } : { href: x64, name: "x86-64" };
    const second = arch === "arm64" ? { href: x64, name: "x86-64" } : { href: arm64, name: "ARM64" };
    return (
      <>
        <a class="button primary" href={first.href}>{label} · {first.name}</a>
        <a class="button quiet" href={second.href}>{label} · {second.name}</a>
        <p class="fine build-note">
          The cross-platform app — pick your architecture (x86-64 or ARM64).{" "}
          <a href={urls.releasesPage}>other platforms</a>
        </p>
      </>
    );
  };

  if (os === "mac") {
    return (
      <>
        <a class="button primary" href={urls.swiftDmg}>Download for macOS</a>
        <a class="button quiet" href={urls.crossDmg}>Cross-platform build</a>
        <p class="fine build-note">
          <strong>Native</strong> is the Swift/AppKit Mac app;{" "}
          <strong>cross-platform</strong> is the same app in Rust (also on Windows &amp; Linux).
        </p>
      </>
    );
  }

  if (os === "windows") {
    return archButtons("Windows", urls.windowsX64, urls.windowsArm64);
  }

  if (os === "linux") {
    return archButtons("Linux", urls.linuxX64, urls.linuxArm64);
  }

  // Unknown / no-JS: a full, static all-platforms list — both arches per
  // portable platform, each an equal link.
  return (
    <>
      <a class="button primary" href={urls.swiftDmg}>Download for macOS</a>
      <p class="fine build-note">
        Cross-platform builds: <a href={urls.crossDmg}>macOS</a> · Windows{" "}
        (<a href={urls.windowsX64}>x86-64</a> / <a href={urls.windowsArm64}>ARM64</a>) · Linux{" "}
        (<a href={urls.linuxX64}>x86-64</a> / <a href={urls.linuxArm64}>ARM64</a>){" "}
        (<a href={urls.releasesPage}>all downloads</a>)
      </p>
    </>
  );
}
