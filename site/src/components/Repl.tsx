import { useEffect, useRef, useState } from "preact/hooks";
import { ensureWasm, WasmCalculator } from "../lib/anzan";

// The live REPL — the real engine (Rust → WASM), not a lookalike. One
// stateful calculator per page visit: `ans`, variables, functions, and the
// mode persist across lines exactly like the app's log and the CLIs.

interface Line {
  input: string;
  ok: boolean;
  text: string; // displayDescription, or the error message
  position?: number; // error caret column, when the engine gives one
}

interface Outcome {
  ok: boolean;
  kind?: string;
  displayDescription?: string;
  rawBlock?: string;
  error?: string;
  position?: number;
}

type Mode = "normal" | "finance" | "programmer";

// Click-to-run starters — each shows off something a float calculator can't.
const EXAMPLES: { label: string; mode: Mode; lines: string[] }[] = [
  { label: "0.1 + 0.2 == 0.3", mode: "normal", lines: ["0.1 + 0.2 == 0.3"] },
  {
    label: "$10,000 + ($15,000 * 5%)",
    mode: "finance",
    lines: ["$10,000 + ($15,000 * 5%)"],
  },
  {
    label: "fact(20)",
    mode: "normal",
    lines: ["fact(n) = if(n <= 1, 1, n * fact(n - 1))", "fact(20)"],
  },
  { label: "man pmt", mode: "normal", lines: ["man pmt"] },
];

export default function Repl() {
  const [lines, setLines] = useState<Line[]>([]);
  const [draft, setDraft] = useState("");
  const [mode, setModeState] = useState<Mode>("normal");
  const [status, setStatus] = useState<"loading" | "ready" | "failed">("loading");
  const calc = useRef<WasmCalculator | null>(null);
  const logRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let alive = true;
    ensureWasm()
      .then(() => {
        if (!alive) return;
        calc.current = new WasmCalculator();
        setStatus("ready");
      })
      .catch(() => alive && setStatus("failed"));
    return () => {
      alive = false;
    };
  }, []);

  useEffect(() => {
    logRef.current?.scrollTo({ top: logRef.current.scrollHeight });
  }, [lines]);

  function setMode(next: Mode) {
    if (!calc.current) return;
    calc.current.mode = next;
    setModeState(next);
  }

  function run(input: string) {
    const engine = calc.current;
    const trimmed = input.trim();
    if (!engine || !trimmed) return;
    const outcome = JSON.parse(engine.evaluate(trimmed)) as Outcome;
    const text = outcome.ok
      ? (outcome.rawBlock ?? outcome.displayDescription ?? "")
      : (outcome.error ?? "error");
    setLines((prev) => [
      ...prev,
      { input: trimmed, ok: outcome.ok, text, position: outcome.position },
    ]);
  }

  function runExample(example: (typeof EXAMPLES)[number]) {
    setMode(example.mode);
    for (const line of example.lines) run(line);
  }

  function onSubmit(event: Event) {
    event.preventDefault();
    run(draft);
    setDraft("");
  }

  if (status === "failed") {
    return <p class="repl-fallback">The live demo needs WebAssembly — try the downloads above instead.</p>;
  }

  return (
    <div class="repl" data-status={status}>
      <div class="repl-toolbar" role="tablist" aria-label="Language mode">
        {(["normal", "finance", "programmer"] as Mode[]).map((m) => (
          <button
            key={m}
            role="tab"
            class={`repl-mode ${mode === m ? "is-active" : ""}`}
            onClick={() => setMode(m)}
            disabled={status !== "ready"}
          >
            {m}
          </button>
        ))}
      </div>
      <div class="repl-log" ref={logRef} aria-live="polite">
        {lines.length === 0 && (
          <p class="repl-hint">
            {status === "loading"
              ? "Loading the engine…"
              : "This is the real engine — the same Rust core the desktop app ships, compiled to WebAssembly. Try an example:"}
          </p>
        )}
        {lines.map((line, i) => (
          <div class="repl-entry" key={i}>
            <div class="repl-in">{line.input}</div>
            {line.ok ? (
              <div class="repl-out">= {line.text}</div>
            ) : (
              <div class="repl-err">
                {line.position != null && (
                  <span class="repl-caret">{" ".repeat(line.position)}^</span>
                )}
                <span>error: {line.text}</span>
              </div>
            )}
          </div>
        ))}
      </div>
      <form class="repl-inputrow" onSubmit={onSubmit}>
        <span class="repl-prompt" aria-hidden="true">
          &gt;
        </span>
        <input
          class="repl-input"
          value={draft}
          onInput={(e) => setDraft((e.target as HTMLInputElement).value)}
          placeholder={status === "ready" ? "Type an expression — Enter to evaluate" : "Loading…"}
          disabled={status !== "ready"}
          autocomplete="off"
          autocapitalize="off"
          spellcheck={false}
          aria-label="Anzan expression"
        />
      </form>
      <div class="repl-examples">
        {EXAMPLES.map((example) => (
          <button
            key={example.label}
            class="repl-chip"
            onClick={() => runExample(example)}
            disabled={status !== "ready"}
          >
            {example.label}
          </button>
        ))}
      </div>
    </div>
  );
}
