// The binding surface: the typed wrapper over the wasm engine. Language
// behavior itself is covered by the shared spec (`npm run spec`); these tests
// pin the boundary — JSON parsed into discriminated unions, session
// statefulness, script halt semantics, and the wasm recursion cap.

import { describe, expect, it } from "vitest";
import {
  reference,
  Calculator,
  StatementAccumulator,
  runScript,
  statements,
  trailingComment,
  usesProgrammerNotation,
  type Mode,
} from "./index.js";

function value(calculator: Calculator, line: string) {
  const outcome = calculator.evaluate(line);
  if (!outcome.ok) throw new Error(`'${line}' failed: ${outcome.error}`);
  return outcome;
}

describe("Calculator", () => {
  it("is exact where floats drift", () => {
    const calculator = new Calculator();
    expect(value(calculator, "0.1 + 0.2 == 0.3").description).toBe("1");
  });

  it("keeps session state across evaluate calls", () => {
    const calculator = new Calculator();
    value(calculator, "x = 3");
    expect(value(calculator, "x^2 + 1").description).toBe("10");
    expect(value(calculator, "ans + 1").description).toBe("11");
  });

  it("reports errors with a position, not a throw", () => {
    const calculator = new Calculator();
    const outcome = calculator.evaluate("1 + ");
    expect(outcome.ok).toBe(false);
    if (outcome.ok) throw new Error("unreachable");
    expect(outcome.error).toContain("unexpected end of expression");
    expect(outcome.position).toBe(3);
  });

  it("switches modes, defaulting to normal", () => {
    const calculator = new Calculator();
    expect(calculator.mode).toBe("normal");
    calculator.mode = "programmer";
    expect(calculator.mode).toBe("programmer");
    // ^ is XOR in programmer mode.
    expect(value(calculator, "0b1100 ^ 0b1010").description).toBe("6");
  });

  it("currency is core grammar: first-class in the default mode, canonical vs display", () => {
    const calculator = new Calculator();
    const outcome = value(calculator, "$10 * 5%");
    expect(outcome.kind).toBe("value");
    expect(outcome.displayDescription).toBe("$0.50");
    expect(outcome.description).toBe('Money(0.5, "USD")');
  });

  it("grouped input echoes grouped in the default mode", () => {
    const calculator = new Calculator();
    const outcome = value(calculator, "138,561 * 9%");
    expect(outcome.description).toBe("12470.49");
    expect(outcome.displayDescription).toBe("12,470.49");
  });

  it("scientific mode echoes plain numbers scientifically; eng snaps to 3", () => {
    const calculator = new Calculator();
    calculator.mode = "scientific";
    const outcome = value(calculator, "123456 * 2");
    // The canonical form stays the plain number — only the echo changes.
    expect(outcome.description).toBe("246912");
    expect(outcome.displayDescription).toBe("2.46912e5");
    calculator.sciStyle = "eng";
    expect(value(calculator, "123456 * 2").displayDescription).toBe("246.912e3");
    // Value-carried display wins over the sci echo.
    expect(value(calculator, "$10 * 5%").displayDescription).toBe("$0.50");
  });

  it("setModeParsing rides the engine's :mode seam", () => {
    const calculator = new Calculator();
    calculator.setModeParsing("scientific eng");
    expect(calculator.mode).toBe("scientific");
    expect(calculator.sciStyle).toBe("eng");
    calculator.setModeParsing("normal");
    expect(calculator.mode).toBe("normal");
  });

  it("the retired finance mode throws the promotion hint", () => {
    const calculator = new Calculator();
    expect(() => {
      calculator.mode = "finance" as unknown as Mode;
    }).toThrow(/currency now works in every mode/);
    expect(() => calculator.setModeParsing("finance")).toThrow(
      /use normal, programmer, or scientific/,
    );
  });

  it("the degree literal converts to radians in every mode", () => {
    const calculator = new Calculator();
    expect(value(calculator, "sin(90°)").description).toBe("1");
    expect(value(calculator, "90° == pi / 2").description).toBe("1");
  });

  it("classifies outcomes by kind", () => {
    const calculator = new Calculator();
    expect(value(calculator, "f(x) = x * 2").kind).toBe("function");
    expect(value(calculator, "data Pt { x: Number }").kind).toBe("data");
    expect(value(calculator, "man pmt").kind).toBe("documentation");
    expect(value(calculator, "# a note").kind).toBe("comment");
  });

  it("completions and documentation come from the engine", () => {
    const calculator = new Calculator();
    expect(calculator.completions("sq").map((c) => c.name)).toContain("sqrt");
    const doc = calculator.documentation("pmt");
    expect(doc).not.toBeNull();
    expect(doc?.signature).toContain("pmt(");
    expect(doc?.summary).not.toBe("");
    expect(doc?.examples.length).toBeGreaterThan(0);
    expect(calculator.documentation("definitely-not-a-function")).toBeNull();
  });

  it("caps deep non-tail recursion with a clean engine error", () => {
    // In wasm there is no stack to grow (no stacker), so a deep non-tail
    // recursion must come back as an engine error — never a thrown
    // RangeError escaping the boundary.
    const calculator = new Calculator();
    value(calculator, "f(n) = if(n == 0, 0, 1 + f(n - 1))");
    const outcome = calculator.evaluate("f(5000)");
    expect(outcome.ok).toBe(false);
    if (outcome.ok) throw new Error("unreachable");
    expect(outcome.error).toContain("nested too deeply");
  });

  it("tail calls do not hit the cap", () => {
    const calculator = new Calculator();
    value(calculator, "tally(n, acc) = if(n == 0, acc, tally(n - 1, acc + 1))");
    expect(value(calculator, "tally(5000, 0)").description).toBe("5000");
  });
});

describe("runScript", () => {
  it("halts at the first error, script-style", () => {
    const calculator = new Calculator();
    const run = calculator.runScript("x = 2\nx * nope\nx + 1");
    expect(run.halted).toBe(true);
    expect(run.results).toHaveLength(2); // x + 1 never ran
    expect(run.results[0]).toMatchObject({ ok: true, description: "2", line: 1 });
    const failure = run.results[1]!;
    expect(failure.ok).toBe(false);
    if (failure.ok) throw new Error("unreachable");
    expect(failure.error).toContain("unknown variable 'nope'");
    expect(failure.statement).toBe("x * nope");
    expect(failure.line).toBe(2);
  });

  it("joins statements across open brackets", () => {
    const run = runScript("sum(\n  1, 2,\n  3\n)");
    expect(run.halted).toBe(false);
    expect(run.results).toHaveLength(1);
    expect(run.results[0]).toMatchObject({ ok: true, description: "6" });
  });

  it("surfaces an unterminated block as the halting result", () => {
    const run = runScript("namespace Broken {\n  x() = 1");
    expect(run.halted).toBe(true);
    expect(run.results).toHaveLength(1);
    const failure = run.results[0]!;
    expect(failure.ok).toBe(false);
    if (failure.ok) throw new Error("unreachable");
    expect(failure.error).toContain("unterminated");
  });
});

describe("StatementAccumulator", () => {
  it("streams: physical lines in, logical statements out", () => {
    const accumulator = new StatementAccumulator();
    expect(accumulator.push("sum(")).toBeNull();
    expect(accumulator.isPending()).toBe(true);
    expect(accumulator.pendingText()).toBe("sum(");
    expect(accumulator.push("1, 2,")).toBeNull();
    const statement = accumulator.push("3)");
    expect(statement).toEqual({ text: "sum( 1, 2, 3)", line: 1 });
    expect(accumulator.isPending()).toBe(false);
    expect(accumulator.finish()).toBeNull();
  });

  it("finish reports an unterminated block", () => {
    const accumulator = new StatementAccumulator();
    accumulator.push("namespace X {");
    const error = accumulator.finish();
    expect(error).not.toBeNull();
    expect(error?.error).toContain("unterminated");
  });

  it("the statements helper splits without evaluating", () => {
    expect(statements("1 + 1\nsum(\n2, 3)").map((s) => s.text)).toEqual([
      "1 + 1",
      "sum( 2, 3)",
    ]);
    expect(() => statements("f(")).toThrow(/unterminated/);
  });
});

describe("display heuristics", () => {
  it("extracts trailing comments", () => {
    expect(trailingComment("5 + 3 # adds")).toBe("adds");
    expect(trailingComment("5 + 3")).toBeUndefined();
  });

  it("detects programmer notation", () => {
    expect(usesProgrammerNotation("0x10 + 1")).toBe(true);
    expect(usesProgrammerNotation("1 + 1")).toBe(false);
  });
});

describe("environment & reference", () => {
  it("lists variables, functions, and ans", () => {
    const calc = new Calculator();
    calc.evaluate("rate = 0.05");
    calc.evaluate("double(x) = x * 2  # twice");
    const env = calc.environment();
    expect(env.variables).toEqual([
      { name: "rate", display: "0.05", canonical: "0.05" },
    ]);
    expect(env.functions[0]?.source).toContain("double(x) = x * 2");
    expect(env.ans.display).toBe("0.05");
  });

  it("exposes the full builtin reference", () => {
    const entries = reference();
    expect(entries.length).toBeGreaterThan(100);
    const pmt = entries.find((e) => e.name === "pmt");
    expect(pmt?.signature).toContain("pmt(rate, nper, pv");
    expect(pmt?.category.length).toBeGreaterThan(0);
  });
});
