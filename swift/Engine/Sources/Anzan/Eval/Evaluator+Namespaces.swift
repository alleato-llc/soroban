// Namespace registration (`namespace Bits { … }`): members register under a
// `prefix::` qualifier, recursing into nested namespaces, with sibling type
// references qualified and constants evaluated eagerly under the home context.

extension Evaluator {
    /// Registers a namespace's members under `prefix::`, recursing into nested
    /// namespaces (`A::B::member`). A data field or function parameter type that
    /// references a sibling TYPE is qualified to the prefix (so dispatch matches
    /// the namespace's instances); a function body resolves its siblings
    /// unqualified at call time via the home-namespace context (see
    /// apply(user:)); a constant evaluates EAGERLY under that context, so it may
    /// reference earlier sibling constants/functions. (docs/MODULES.md)
    func registerNamespace(_ prefix: String, members: [Expression],
                           in environment: EvaluationEnvironment, depth: Int,
                           typeScope enclosing: [String: String] = [:]) throws {
        // This level's type names, mapped to their qualified form, ON TOP of the
        // enclosing scope — so a member may name a sibling OR a parent's type
        // unqualified; nesting shadows the parent.
        var scope = enclosing
        for member in members {
            switch member {
            case .dataDefinition(let typeName, _):
                scope[typeName.lowercased()] = "\(prefix)::\(typeName)"
            case .functionDefinition, .assignment, .namespaceDefinition: break
            default:
                throw EngineError.domainError(message:
                    "namespace \(prefix) holds data, function, constant, and nested namespace declarations")
            }
        }
        for member in members {
            switch member {
            case .dataDefinition(let typeName, let fields):
                let qualified = "\(prefix)::\(typeName)"
                guard environment.function(named: qualified) == nil else {
                    throw EngineError.domainError(message: "'\(qualified)' is already a function")
                }
                let qualifiedFields = fields.map {
                    DataField(name: $0.name, type: $0.type.qualified(using: scope))
                }
                environment.define(DataType(name: qualified, fields: qualifiedFields, source: ""))
            case .functionDefinition(let funcName, let parameters, let body):
                let qualified = "\(prefix)::\(funcName)"
                guard environment.dataType(named: qualified) == nil else {
                    throw EngineError.domainError(message: "'\(qualified)' is already a data type")
                }
                let qualifiedParams = parameters.map {
                    Parameter(name: $0.name, type: $0.type?.qualified(using: scope))
                }
                environment.define(UserFunction(name: qualified, parameters: qualifiedParams, body: body, source: ""))
            case .assignment(let varName, let valueExpr):
                let qualified = "\(prefix)::\(varName)"
                guard environment.function(named: qualified) == nil,
                      environment.dataType(named: qualified) == nil else {
                    throw EngineError.domainError(message: "'\(qualified)' is already defined")
                }
                environment.enterNamespace(prefix)
                let value: Value
                do { value = try evaluate(valueExpr, in: environment, locals: [:], depth: depth) }
                catch { environment.leaveNamespace(); throw error }
                environment.leaveNamespace()
                environment[qualified] = value
            case .namespaceDefinition(let innerName, let innerMembers):
                try registerNamespace("\(prefix)::\(innerName)", members: innerMembers,
                                      in: environment, depth: depth, typeScope: scope)
            default:
                break
            }
        }
    }
}
