import { useEffect, useState } from "preact/hooks";

type Mode = "light" | "dark";

/** Mirrors the app's light/dark pairing. The inline script in Layout.astro
 *  resolves system preference before first paint; this toggle overrides it
 *  and remembers the choice. */
export default function ThemeToggle() {
  const [mode, setMode] = useState<Mode>("light");

  useEffect(() => {
    setMode((document.documentElement.dataset.theme as Mode) ?? "light");
  }, []);

  const flip = () => {
    const next: Mode = mode === "light" ? "dark" : "light";
    document.documentElement.dataset.theme = next;
    localStorage.setItem("soroban-theme", next);
    setMode(next);
  };

  return (
    <button
      class="theme-toggle"
      onClick={flip}
      aria-label={`Switch to ${mode === "light" ? "dark" : "light"} theme`}
      title={`Switch to ${mode === "light" ? "dark" : "light"} theme`}
    >
      {mode === "light" ? "◐" : "◑"}
    </button>
  );
}
