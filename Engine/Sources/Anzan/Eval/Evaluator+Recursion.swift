import Darwin      // pthread stack introspection for the recursion guard
import Foundation  // Thread + DispatchSemaphore for stack segmentation

/// How user-function application recurses — the whole story in one place:
///
/// Recursion is bounded by MEMORY and a sanity cap, never by whichever
/// thread happened to call evaluate(). TAIL calls loop at constant stack
/// (`apply(user:)` + `tailStep`); non-tail recursion grows the stack and,
/// when the current thread runs low, CONTINUES on a fresh thread with a
/// roomy stack (stack segmentation) — the caller blocks until that segment
/// returns, so the engine's single-threaded discipline is fully preserved.
/// This replaced a fixed depth (40!) that was sized for Swift Testing's
/// ~512 KB cooperative stacks and strangled legitimate fib() everywhere.
extension Evaluator {
    private static let stackHeadroom: UInt = 128 * 1024
    private static let segmentStackSize = 16 << 20 // 16 MB per segment
    /// Sanity cap: a missing base case errors (with a hint) instead of
    /// chewing memory forever. ~10k frames is far beyond honest recursion.
    private static let maxCallDepth = 10_000
    /// Tail-call iteration cap: a tail loop uses CONSTANT stack, so without
    /// this a base-case-less TAIL recursion would spin forever. Generous
    /// enough for honest iteration; typos surface in about a second.
    private static let maxTailIterations = 1_000_000

    /// True when the current thread's unused stack is below the headroom.
    /// Stacks grow downward from pthread_get_stackaddr_np's base address.
    static func nearStackLimit() -> Bool {
        var probe: UInt8 = 0
        let current = withUnsafePointer(to: &probe) { UInt(bitPattern: $0) }
        let base = UInt(bitPattern: pthread_get_stackaddr_np(pthread_self()))
        let size = UInt(pthread_get_stacksize_np(pthread_self()))
        guard base >= size, current >= base - size else { return true } // be safe
        return current - (base - size) < Self.stackHeadroom
    }

    /// Runs `body` on a new thread with `segmentStackSize` of stack and
    /// blocks until it finishes. Safety: the spawning thread WAITS, so the
    /// non-Sendable evaluation state is never touched concurrently — the
    /// @unchecked box only ferries it across the (strictly serialized) hop.
    private static func continueOnFreshStack<T>(_ body: () throws -> T) throws -> T {
        let box = StackSegmentBox<T>()
        let semaphore = DispatchSemaphore(value: 0)
        try withoutActuallyEscaping(body) { escapable in
            box.run = escapable
            let thread = Thread {
                box.result = Result { try box.run!() }
                box.run = nil
                semaphore.signal()
            }
            thread.stackSize = Self.segmentStackSize
            thread.start()
            semaphore.wait()
        }
        return try box.result!.get()
    }

    /// One step of tail-aware body evaluation: either a finished value, or
    /// "now call THIS function with THESE arguments" — which `apply(user:)`
    /// turns into a loop iteration instead of a stack frame.
    private enum TailStep {
        case value(Value)
        case call(UserFunction, [Value], captures: [String: Value])
    }

    func apply(user function: UserFunction, arguments: [Value],
               captures: [String: Value],
               in environment: EvaluationEnvironment, depth: Int) throws -> Value {
        var function = function
        var arguments = arguments
        var captures = captures
        var iterations = 0

        // TAIL-CALL OPTIMIZATION: a recursive call in tail position (the
        // whole result of the taken if() branch) loops here at CONSTANT
        // stack — sumTo(n, acc) = if(n <= 0, acc, sumTo(n - 1, acc + n))
        // runs to any depth. Non-tail recursion still stacks, hopping to
        // fresh segments when the thread runs low.
        while true {
            guard function.parameters.count == arguments.count else {
                throw EngineError.arityMismatch(function: function.name,
                                                expected: "\(function.parameters.count)",
                                                got: arguments.count)
            }
            guard depth < Self.maxCallDepth, iterations < Self.maxTailIterations else {
                throw EngineError.domainError(message:
                    "function calls nested too deeply — if \(function.name)() is recursive, "
                    + "check its base case — e.g. factorial is "
                    + "fact2(n) = if(n <= 1, 1, n * fact2(n - 1)), "
                    + "fibonacci is fib(n) = if(n <= 2, 1, fib(n - 1) + fib(n - 2))")
            }
            // Out of stack ≠ out of budget: hop to a fresh segment.
            if Self.nearStackLimit() {
                let function = function, arguments = arguments, captures = captures
                return try Self.continueOnFreshStack {
                    try self.apply(user: function, arguments: arguments,
                                   captures: captures, in: environment, depth: depth)
                }
            }
            // Parameters shadow captures, which shadow globals.
            var locals = captures
            for (parameter, argument) in zip(function.parameters, arguments) {
                locals[parameter.name] = argument
            }
            // A namespaced member resolves siblings unqualified while its body
            // runs (home-context); a plain function pushes nil. Per-iteration so
            // a tail call into another namespace sees the right home; balanced
            // on throw. Nested (non-tail) calls push their own in their apply().
            environment.enterNamespace(Self.homeNamespace(of: function.name))
            let step: TailStep
            do {
                step = try tailStep(function.body, in: environment, locals: locals, depth: depth + 1)
            } catch {
                environment.leaveNamespace()
                throw error
            }
            environment.leaveNamespace()
            switch step {
            case .value(let value):
                return value
            case .call(let next, let nextArguments, let nextCaptures):
                function = next
                arguments = nextArguments
                captures = nextCaptures
                iterations += 1
            }
        }
    }

    /// The namespace a qualified name lives in — `Bits::area` → `Bits`, a plain
    /// name → nil. (Flat namespaces for now; the prefix before the first `::`.)
    static func homeNamespace(of name: String) -> String? {
        guard let separator = name.range(of: "::") else { return nil }
        return String(name[..<separator.lowerBound])
    }

    /// Walks tail positions: through the taken branch of if(), down to a
    /// call. A call resolving to a USER function (scoped λ cell, log
    /// function, or a function-valued variable/lambda) becomes a TailStep
    /// .call; registry builtins and every other shape evaluate normally.
    /// Resolution order mirrors `call(name:)` exactly.
    private func tailStep(_ expression: Expression, in environment: EvaluationEnvironment,
                          locals: [String: Value], depth: Int) throws -> TailStep {
        switch expression {
        case .conditional(let conditionExpr, let thenExpr, let elseExpr):
            let condition = try evaluate(conditionExpr, in: environment, locals: locals, depth: depth)
            let branch = try condition.asNumber(for: "the if() condition").isZero
                ? elseExpr : thenExpr
            return try tailStep(branch, in: environment, locals: locals, depth: depth)

        case .call(let name, let argumentExprs) where !registry.contains(name: name):
            let arguments = try arguments(of: argumentExprs, in: environment,
                                          locals: locals, depth: depth)
            // Mirror call(name:)'s namespace-sibling resolution (home-context).
            if let ns = environment.currentNamespace, !name.contains("::") {
                let qualified = "\(ns)::\(name)"
                if let function = environment.function(named: qualified) {
                    return .call(function, arguments, captures: [:])
                }
                if let type = environment.dataType(named: qualified) {
                    return .value(try construct(type, arguments: arguments))
                }
            }
            if let scoped = resolveScopedFunction?(name) {
                return .call(scoped, arguments, captures: [:])
            }
            let overloads = environment.overloads(named: name)
            if !overloads.isEmpty {
                let chosen = try selectOverload(name: name, arguments: arguments, from: overloads)
                return .call(chosen, arguments, captures: [:])
            }
            if let type = resolveScopedDataType?(name) ?? environment.dataType(named: name) {
                return .value(try construct(type, arguments: arguments))
            }
            if case .function(let fn) = locals[name] ?? environment[name] {
                switch fn.kind {
                case .lambda(let parameters, let body):
                    return .call(UserFunction(name: name,
                                              parameters: parameters.map { Parameter(name: $0) },
                                              body: body, source: ""),
                                 arguments, captures: fn.captures)
                case .user(let userName):
                    if let function = environment.function(named: userName) {
                        return .call(function, arguments, captures: [:])
                    }
                    throw EngineError.unknownFunction(name: userName)
                case .builtin(let builtinName):
                    return .value(try registry.call(name: builtinName,
                                                    arguments: arguments) { inner, args in
                        try self.apply(function: inner, arguments: args,
                                       in: environment, depth: depth)
                    })
                }
            }
            // Imported namespaces — the final fallback (mirrors call(name:)).
            if let qualified = environment.importedName(name) {
                if let function = environment.function(named: qualified) {
                    return .call(function, arguments, captures: [:])
                }
                if let type = environment.dataType(named: qualified) {
                    return .value(try construct(type, arguments: arguments))
                }
            }
            // A qualified builtin (`Finance::pmt`).
            if let bare = registry.resolveQualified(name) {
                return .value(try registry.call(name: bare, arguments: arguments) { inner, args in
                    try self.apply(function: inner, arguments: args, in: environment, depth: depth)
                })
            }
            throw EngineError.unknownFunction(name: name)

        default:
            return .value(try evaluate(expression, in: environment,
                                       locals: locals, depth: depth))
        }
    }
}

/// Ferries one evaluation segment across a thread hop (see
/// `Evaluator.continueOnFreshStack` — the hop is strictly serialized, the
/// spawner blocks, hence @unchecked).
private final class StackSegmentBox<U>: @unchecked Sendable {
    var run: (() throws -> U)?
    var result: Result<U, any Error>?
}
