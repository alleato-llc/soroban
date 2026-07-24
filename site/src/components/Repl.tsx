import { useEffect, useMemo, useRef, useState } from "preact/hooks";
import { ensureWasm, WasmCalculator, reference } from "../lib/anzan";

// The live REPL — the real engine (Rust → WASM), shaped like the desktop
// apps: a menu bar (About / Open / Save As / Examples), the mode badge
// cycler (# normal · π scientific · </> programmer — the app's input-bar
// affordance), and the ENV / ? companion panels. One stateful calculator
// per page visit; `ans`, variables, functions, and the mode persist.

interface Line {
  input: string;
  ok: boolean;
  text: string;
  position?: number;
}

interface Outcome {
  ok: boolean;
  kind?: string;
  displayDescription?: string;
  rawBlock?: string;
  error?: string;
  position?: number;
}

interface ScriptResult {
  results: (Outcome & { statement?: string; line?: number })[];
  halted: boolean;
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

type Mode = "normal" | "scientific" | "programmer";
type Panel = "none" | "env" | "help";

// The app's input-bar mode affordance: # Normal · π Scientific · </> Programmer.
const MODE_CYCLE: Mode[] = ["normal", "scientific", "programmer"];
const MODE_BADGE: Record<Mode, string> = {
  normal: "#",
  scientific: "π",
  programmer: "</>",
};

// The Swift app's Examples menu, verbatim (CalculatorSession.welcomeCategories)
// — grouped for the menu; the flattened pool feeds the shuffled welcome picks.
const EXAMPLE_CATEGORIES: { name: string; examples: string[] }[] = [
  {
    // The flagship: data types + recursion + namespaces + finance, one line.
    name: "Showcase",
    examples: [
      "namespace Cash { data Change { quarters: Number, dimes: Number, nickels: Number, pennies: Number }; coins(c, d) = if(c < d, 0, 1 + coins(c - d, d)); makeChange(c) = Change(quarters: coins(c, 25), dimes: coins(mod(c, 25), 10), nickels: coins(mod(mod(c, 25), 10), 5), pennies: coins(mod(mod(mod(c, 25), 10), 5), 1)); changeForDollar(cost) = makeChange((1 - cost) * 100) }",
      "Cash::changeForDollar(0.95)",
    ],
  },
  {
    name: "Higher-order",
    examples: [
      "map(n -> n * n, filter(x -> mod(x, 2) == 0, seq(1, 20)))",
      "reduce((a, b) -> a * b, seq(1, 10), 1)",
      "sum(map(x -> x^2, seq(1, 10)))",
      "len(filter(x -> x > 5, [3, 7, 2, 9, 5, 11]))",
    ],
  },
  {
    name: "Reductions",
    examples: ["∑_i=1^100(1 / i^2)", "∏_i=1^10(i)"],
  },
  {
    name: "Finance",
    examples: [
      "pmt(0.0425/12, 360, 450000)",
      "round(100000 * (1 + 0.05/12)^(12 * 10), 2)",
      "npv(0.1, -1000, 300, 400, 500, 600)",
      "fv(0.06, 10, -1200)",
      "ipmt(0.05/12, 1, 360, 200000)",
    ],
  },
  {
    name: "Statistics",
    examples: [
      "stdev(82, 91, 77, 88, 64, 95)",
      "percentile(seq(1, 100), 0.9)",
      "median(seq(1, 99))",
      "forecast(8, 1, 2, 3, 4, 2, 4, 6, 8)",
    ],
  },
  {
    name: "Combinatorics",
    examples: [
      "fact(52) / (fact(5) * fact(47))",
      "choose(52, 5)",
      "perm(10, 3)",
      "lcm(12, 18)",
    ],
  },
  {
    name: "Structures",
    examples: [
      "sort([5, 2, 8, 1, 9, 3])",
      "unique([3, 1, 4, 1, 5, 9, 2, 6, 5, 3])",
      "keys({alpha: 1, beta: 2, gamma: 3})",
      "concat([1, 2, 3], [4, 5, 6])",
      '{name: "Ada", born: 1815}.born',
    ],
  },
  {
    name: "JSON & data types",
    examples: [
      'toJson({name: "Ada", scores: [91, 88, 95]})',
      'fromJson("{\\"x\\": 3, \\"y\\": 4}")',
      "data Point { x: Number, y: Number }",
    ],
  },
  {
    name: "Definitions & logic",
    examples: [
      "compound(p, r, n) = p * (1 + r)^n",
      'if(gcd(17, 5) == 1, "coprime", "shares a factor")',
    ],
  },
  {
    name: "Programmer",
    examples: ["0xFF + 0b1010", 'fromBase("FF", 16)', "bitXor(12, 10)", "log(2, 1024)"],
  },
  {
    name: "Dates",
    examples: ["edate(today(), 6)", "networkdays(today(), today() + 30)"],
  },
  {
    name: "Scientific",
    examples: ["atan2(1, 1) * 4", "exp(1)"],
  },
  {
    name: "Simple",
    examples: ["sqrt(3^2 + 4^2)", "2 ^ 64", "x = 12 * 80.5", "ans * 1.0825"],
  },
];
// Showcase is menu-only — its namespace one-liner is too long for a
// welcome suggestion line.
const EXAMPLE_POOL = EXAMPLE_CATEGORIES.filter((c) => c.name !== "Showcase").flatMap(
  (c) => c.examples,
);

function shuffled<T>(items: T[]): T[] {
  const copy = [...items];
  for (let i = copy.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [copy[i], copy[j]] = [copy[j], copy[i]];
  }
  return copy;
}

export default function Repl() {
  const [lines, setLines] = useState<Line[]>([]);
  const [draft, setDraft] = useState("");
  const [mode, setModeState] = useState<Mode>("normal");
  const [panel, setPanel] = useState<Panel>("none");
  const [env, setEnv] = useState<Env | null>(null);
  const [search, setSearch] = useState("");
  const [examplesOpen, setExamplesOpen] = useState(false);
  const [aboutOpen, setAboutOpen] = useState(false);
  const [status, setStatus] = useState<"loading" | "ready" | "failed">("loading");
  const calc = useRef<WasmCalculator | null>(null);
  const logRef = useRef<HTMLDivElement>(null);
  const fileRef = useRef<HTMLInputElement>(null);

  // The welcome picks — the app shuffles its pool on launch; ten here.
  const welcome = useMemo(() => shuffled(EXAMPLE_POOL).slice(0, 10), []);

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

  // The reference is static — parse once when ready.
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

  function cycleMode() {
    if (!calc.current) return;
    const next = MODE_CYCLE[(MODE_CYCLE.indexOf(mode) + 1) % MODE_CYCLE.length];
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

  // Examples are written in the canonical (normal) grammar — running one
  // under another dialect misreads its glyphs (in programmer mode `^` is
  // XOR, so `(1+r)^n` becomes a bitXor error). Snap the mode badge to
  // normal first: visible on the badge, never a silent switch.
  function runExample(example: string) {
    if (mode !== "normal" && calc.current) {
      calc.current.mode = "normal";
      setModeState("normal");
    }
    run(example);
  }

  function onSubmit(event: Event) {
    event.preventDefault();
    run(draft);
    setDraft("");
  }

  function togglePanel(which: Exclude<Panel, "none">) {
    setPanel((current) => (current === which ? "none" : which));
  }

  function closeMenus() {
    setExamplesOpen(false);
    setAboutOpen(false);
  }

  // Open — run a local .anzan script through the session (halts at the
  // first error, like the CLIs).
  function openFile(file: File) {
    file.text().then((source) => {
      const engine = calc.current;
      if (!engine) return;
      const script = JSON.parse(engine.runScript(source)) as ScriptResult;
      setLines((prev) => [
        ...prev,
        ...script.results
          .filter((r) => !(r.statement ?? "").startsWith("#"))
          .map((r) => ({
            input: r.statement ?? "",
            ok: r.ok,
            text: r.ok
              ? (r.rawBlock ?? r.displayDescription ?? "")
              : (r.error ?? "error"),
            position: r.position,
          })),
      ]);
    });
  }

  // Save As — the session's inputs as a runnable .anzan script.
  function saveAs() {
    const source =
      "#!/usr/bin/env soroban\n# Saved from soroban.alleato.dev\n" +
      lines.map((l) => l.input).join("\n") +
      "\n";
    const blob = new Blob([source], { type: "text/plain" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = "session.anzan";
    a.click();
    URL.revokeObjectURL(url);
  }

  if (status === "failed") {
    return <p class="repl-fallback">The live demo needs WebAssembly — try the downloads above instead.</p>;
  }

  const ready = status === "ready";

  // The Examples menu groups, rendered into the menu-bar dropdown.
  const exampleGroups = EXAMPLE_CATEGORIES.map((group) => (
    <div key={group.name}>
      <h3>{group.name}</h3>
      {group.examples.map((example) => (
        <button
          key={example}
          class="repl-dropdown-item"
          role="menuitem"
          onClick={() => {
            closeMenus();
            runExample(example);
          }}
        >
          {example}
        </button>
      ))}
    </div>
  ));

  const tree = (
    <div class="repl" data-status={status}>
      <div class="repl-menubar">
        <button class="repl-menu-btn" onClick={() => setAboutOpen(true)} disabled={!ready}>
          About
        </button>
        <button class="repl-menu-btn" onClick={() => fileRef.current?.click()} disabled={!ready}>
          Open…
        </button>
        <button
          class="repl-menu-btn"
          onClick={saveAs}
          disabled={!ready || lines.length === 0}
          title="Download the session as a runnable .anzan script"
        >
          Save As…
        </button>
        <div class="repl-menu-wrap">
          <button
            class={`repl-menu-btn ${examplesOpen ? "is-active" : ""}`}
            onClick={() => setExamplesOpen((open) => !open)}
            disabled={!ready}
            aria-expanded={examplesOpen}
          >
            Examples ▾
          </button>
          {examplesOpen && (
            <div class="repl-dropdown" role="menu">
              {exampleGroups}
            </div>
          )}
        </div>
        <div class="repl-tools">
          <button
            class={`repl-tool ${panel === "env" ? "is-active" : ""}`}
            onClick={() => togglePanel("env")}
            disabled={!ready}
            aria-pressed={panel === "env"}
            title="The session's variables, functions, and data types"
          >
            ENV
          </button>
          <button
            class={`repl-tool ${panel === "help" ? "is-active" : ""}`}
            onClick={() => togglePanel("help")}
            disabled={!ready}
            aria-pressed={panel === "help"}
            title="Every built-in function, searchable"
          >
            ?
          </button>
        </div>
        <input
          ref={fileRef}
          type="file"
          accept=".anzan,text/plain"
          hidden
          onChange={(e) => {
            const file = (e.target as HTMLInputElement).files?.[0];
            if (file) openFile(file);
            (e.target as HTMLInputElement).value = "";
          }}
        />
      </div>
      <div class="repl-body">
        <div class="repl-log" ref={logRef} aria-live="polite">
          {lines.length === 0 && (
            <div class="repl-welcome">
              <p class="repl-hint">
                {status === "loading"
                  ? "Loading the engine…"
                  : "This is the real engine — the same Rust core the desktop apps ship, compiled to WebAssembly. Try one:"}
              </p>
              {ready &&
                welcome.map((example) => (
                  <button key={example} class="repl-welcome-line" onClick={() => runExample(example)}>
                    {example}
                  </button>
                ))}
            </div>
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
          placeholder={ready ? "Type an expression — Enter to evaluate" : "Loading…"}
          disabled={!ready}
          autocomplete="off"
          autocapitalize="off"
          spellcheck={false}
          aria-label="Anzan expression"
        />
        <button
          type="button"
          class="repl-mode is-active"
          onClick={cycleMode}
          disabled={!ready}
          title="Cycle the language mode — # normal · π scientific · </> programmer"
        >
          {MODE_BADGE[mode]} {mode}
        </button>
      </form>
      {(examplesOpen || aboutOpen) && (
        <button
          class="repl-backdrop"
          aria-label="Close"
          onClick={() => {
            setExamplesOpen(false);
            setAboutOpen(false);
          }}
        />
      )}
      {aboutOpen && (
        <div class="repl-about" role="dialog" aria-label="About">
          <h3>Soroban・算盤 in the browser</h3>
          <p>
            This REPL runs the <strong>same exact-decimal Rust engine</strong>{" "}
            the desktop apps ship, compiled to WebAssembly — 50 significant
            digits, no floating-point drift. Sessions live in your tab;
            nothing is sent anywhere.
          </p>
          <p>
            <a href="/anzan">Language spec</a> ·{" "}
            <a href="https://github.com/alleato-llc/soroban">Source</a> ·{" "}
            <a href="#top">Downloads</a>
          </p>
          <button class="repl-menu-btn" onClick={() => setAboutOpen(false)}>
            Close
          </button>
        </div>
      )}
    </div>
  );

  return tree;
}
