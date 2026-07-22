import { useEffect, useMemo, useRef, useState } from "preact/hooks";
import { ensureWasm, WasmCalculator, reference } from "../lib/anzan";

// The live REPL — the real engine (Rust → WASM), not a lookalike. One
// stateful calculator per page visit: `ans`, variables, functions, and the
// mode persist across lines exactly like the app's log. The toolbar mirrors
// the desktop apps' affordances: mode picker, environment inspector, help.

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

interface Env {
  ans: { display: string };
  variables: { name: string; display: string }[];
  functions: { name: string; source: string }[];
  dataTypes: { name: string; declaration: string }[];
}

interface RefEntry {
  name: string;
  category: string;
  signature: string;
  summary: string;
}

type Mode = "normal" | "finance" | "programmer";
type Panel = "none" | "env" | "help";

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
  const [panel, setPanel] = useState<Panel>("none");
  const [env, setEnv] = useState<Env | null>(null);
  const [search, setSearch] = useState("");
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

  // The inspector refreshes with every evaluated line while open.
  useEffect(() => {
    if (panel === "env" && calc.current) {
      setEnv(JSON.parse(calc.current.environment()) as Env);
    }
  }, [panel, lines]);

  // The reference is static — load once, on first open.
  const refEntries = useMemo<RefEntry[]>(
    () => (status === "ready" ? (JSON.parse(reference()) as RefEntry[]) : []),
    [status],
  );
  const filteredRef = useMemo(() => {
    const q = search.trim().toLowerCase();
    if (!q) return refEntries;
    return refEntries.filter(
      (e) => e.name.toLowerCase().includes(q) || e.summary.toLowerCase().includes(q),
    );
  }, [refEntries, search]);

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

  function togglePanel(which: Exclude<Panel, "none">) {
    setPanel((current) => (current === which ? "none" : which));
  }

  if (status === "failed") {
    return <p class="repl-fallback">The live demo needs WebAssembly — try the downloads above instead.</p>;
  }

  return (
    <div class="repl" data-status={status}>
      <div class="repl-toolbar">
        <div role="tablist" aria-label="Language mode" class="repl-modes">
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
        <div class="repl-tools">
          <button
            class={`repl-tool ${panel === "env" ? "is-active" : ""}`}
            onClick={() => togglePanel("env")}
            disabled={status !== "ready"}
            aria-pressed={panel === "env"}
            title="The session's variables, functions, and data types"
          >
            𝑥 environment
          </button>
          <button
            class={`repl-tool ${panel === "help" ? "is-active" : ""}`}
            onClick={() => togglePanel("help")}
            disabled={status !== "ready"}
            aria-pressed={panel === "help"}
            title="Every built-in function, searchable"
          >
            ? help
          </button>
        </div>
      </div>
      <div class="repl-body">
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
        {panel === "env" && (
          <aside class="repl-panel" aria-label="Environment inspector">
            <h3>ans</h3>
            <div class="repl-row mono">{env?.ans.display ?? "0"}</div>
            <h3>Variables</h3>
            {env && env.variables.length > 0 ? (
              env.variables.map((v) => (
                <div class="repl-row mono" key={v.name}>
                  {v.name} = {v.display}
                </div>
              ))
            ) : (
              <div class="repl-empty">none yet — try x = 3</div>
            )}
            <h3>Functions</h3>
            {env && env.functions.length > 0 ? (
              env.functions.map((f) => (
                <div class="repl-row mono" key={f.name}>
                  {f.source}
                </div>
              ))
            ) : (
              <div class="repl-empty">none yet — try f(x) = x * 2</div>
            )}
            {env && env.dataTypes.length > 0 && (
              <>
                <h3>Data types</h3>
                {env.dataTypes.map((t) => (
                  <div class="repl-row mono" key={t.name}>
                    {t.declaration}
                  </div>
                ))}
              </>
            )}
          </aside>
        )}
        {panel === "help" && (
          <aside class="repl-panel" aria-label="Function reference">
            <input
              class="repl-search"
              value={search}
              onInput={(e) => setSearch((e.target as HTMLInputElement).value)}
              placeholder={`Search ${refEntries.length} built-ins…`}
              aria-label="Search built-ins"
            />
            {filteredRef.map((entry, i) => (
              <div class="repl-ref" key={entry.name + i}>
                {(i === 0 || filteredRef[i - 1].category !== entry.category) && (
                  <h3>{entry.category}</h3>
                )}
                <button
                  class="repl-row mono repl-ref-sig"
                  title={entry.summary}
                  onClick={() => run(`man ${entry.name}`)}
                >
                  {entry.signature}
                </button>
              </div>
            ))}
          </aside>
        )}
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
