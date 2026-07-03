/// All errors the engine surfaces to the UI. Positions are character offsets
/// into the source line so the UI can point at the problem.
public enum EngineError: Error, Equatable, Sendable {
    case lexError(message: String, position: Int)
    case parseError(message: String, position: Int)
    case divisionByZero
    case unknownVariable(name: String)
    case unknownFunction(name: String)
    case arityMismatch(function: String, expected: String, got: Int)
    case domainError(message: String)
}

extension EngineError: CustomStringConvertible {
    public var description: String {
        switch self {
        case .lexError(let message, let position):
            return "syntax error at column \(position + 1): \(message)"
        case .parseError(let message, let position):
            return "parse error at column \(position + 1): \(message)"
        case .divisionByZero:
            return "division by zero"
        case .unknownVariable(let name):
            return "unknown variable '\(name)'"
        case .unknownFunction(let name):
            return "unknown function '\(name)'"
        case .arityMismatch(let function, let expected, let got):
            return "\(function)() expects \(expected) argument\(expected == "1" ? "" : "s"), got \(got)"
        case .domainError(let message):
            return message
        }
    }

    /// Column to point a caret at, when the error is positional.
    public var position: Int? {
        switch self {
        case .lexError(_, let position), .parseError(_, let position):
            return position
        default:
            return nil
        }
    }
}
