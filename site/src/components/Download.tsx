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
    const primary = arch === "arm64" ? urls.windowsArm64 : urls.windowsX64;
    const other = arch === "arm64" ? urls.windowsX64 : urls.windowsArm64;
    const otherLabel = arch === "arm64" ? "x86-64" : "ARM64";
    return (
      <>
        <a class="button primary" href={primary}>Download for Windows</a>
        <p class="fine build-note">
          The cross-platform app ({arch === "arm64" ? "ARM64" : "x86-64"}).{" "}
          <a href={other}>{otherLabel} build</a> ·{" "}
          <a href={urls.releasesPage}>other platforms</a>
        </p>
      </>
    );
  }

  if (os === "linux") {
    const primary = arch === "arm64" ? urls.linuxArm64 : urls.linuxX64;
    const other = arch === "arm64" ? urls.linuxX64 : urls.linuxArm64;
    const otherLabel = arch === "arm64" ? "x86-64" : "ARM64";
    return (
      <>
        <a class="button primary" href={primary}>Download for Linux</a>
        <p class="fine build-note">
          The cross-platform app ({arch === "arm64" ? "ARM64" : "x86-64"}).{" "}
          <a href={other}>{otherLabel} build</a> ·{" "}
          <a href={urls.releasesPage}>other platforms</a>
        </p>
      </>
    );
  }

  // Unknown / no-JS: a full, static all-platforms list.
  return (
    <>
      <a class="button primary" href={urls.swiftDmg}>Download for macOS</a>
      <p class="fine build-note">
        Cross-platform builds:{" "}
        <a href={urls.crossDmg}>macOS</a> ·{" "}
        <a href={urls.windowsX64}>Windows</a> ·{" "}
        <a href={urls.linuxX64}>Linux</a>{" "}
        (<a href={urls.releasesPage}>all downloads</a>)
      </p>
    </>
  );
}
