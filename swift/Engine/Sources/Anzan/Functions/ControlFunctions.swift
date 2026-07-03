/// Control expressions — functions whose CALLS double as interactive grid
/// controls (see Slider.swift). Evaluation is ordinary and pure, so
/// workbooks behave identically headlessly: `slider(v, lo, hi)` is just `v`
/// clamped into range.
let controlFunctions: [BuiltinFunction] = [
    BuiltinFunction(
        name: "slider",
        category: .controls,
        signature: "slider(value, min, max, step?)",
        summary: "A what-if slider. In a grid cell — ideally a definition like rate = slider(0.08, 0, 0.2) — it renders as a draggable control; dragging rewrites the value in place and recalculates everything that reads it. Evaluates to the value, clamped into min…max. Step defaults to (max−min)/100.",
        examples: ["slider(5, 0, 10)", "slider(15, 0, 10)", "slider(0.5, 0, 1, 0.25)"],
        arity: 3...4,
        apply: { arguments in
            let minimum = arguments[1], maximum = arguments[2]
            guard minimum < maximum else {
                throw EngineError.domainError(message: "slider() needs min < max")
            }
            if arguments.count == 4, !(arguments[3] > .zero) {
                throw EngineError.domainError(message: "slider() step must be positive")
            }
            return min(max(arguments[0], minimum), maximum)
        }),

    BuiltinFunction(
        name: "stepper",
        category: .controls,
        signature: "stepper(value, min, max, step?)",
        summary: "A discrete what-if control: − and + buttons move the value by step (default 1), clamped into min…max. n = stepper(5, 1, 20) in a cell renders the control; formulas read n.",
        examples: ["stepper(5, 1, 20)", "stepper(2.5, 0, 10, 2.5)"],
        arity: 3...4,
        apply: { arguments in
            let minimum = arguments[1], maximum = arguments[2]
            guard minimum < maximum else {
                throw EngineError.domainError(message: "stepper() needs min < max")
            }
            if arguments.count == 4, !(arguments[3] > .zero) {
                throw EngineError.domainError(message: "stepper() step must be positive")
            }
            return min(max(arguments[0], minimum), maximum)
        }),

    BuiltinFunction(
        name: "checkbox",
        category: .controls,
        signature: "checkbox(state)",
        summary: "A toggle: flag = checkbox(true) renders as a checkbox; clicking flips it in place. Evaluates to 1 or 0 (the engine's truth values), so if(flag, …, …) and sum(…) over checkbox ranges both work.",
        examples: ["checkbox(true)", "checkbox(false)", "if(checkbox(true), 10, 20)"],
        arity: 1...1,
        apply: { arguments in
            arguments[0].isZero ? .zero : .one
        }),

    BuiltinFunction(
        name: "dropdown",
        category: .controls,
        signature: "dropdown(value, [options])",
        summary: "A picker: region = dropdown(\"EU\", [\"EU\", \"US\", \"APAC\"]) renders as a menu; choosing rewrites the value in place. Evaluates to the selected value — strings compare with ==, numeric options behave as numbers.",
        examples: ["dropdown(\"EU\", [\"EU\", \"US\", \"APAC\"])", "dropdown(5, [1, 5, 10])"],
        arity: 2...2,
        applyValues: { arguments in
            guard case .array = arguments[1] else {
                throw EngineError.domainError(
                    message: "dropdown() wants (value, [options]) — got \(arguments[1].kindName) second")
            }
            return arguments[0]
        }),
]
