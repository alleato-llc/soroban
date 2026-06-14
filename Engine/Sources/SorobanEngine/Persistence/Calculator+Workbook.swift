import Anzan
/// The workbook half of session restore. Lives on the HOSTING side of the
/// module boundary: Calculator (Anzan) knows nothing about Workbook — this
/// extension is the one place the two meet.
extension Calculator {
    /// Replaces the session's definitions and variables from a workbook.
    /// Order matters: namespaces (which register their qualified members) →
    /// imports (which need those namespaces) → data types → functions →
    /// variables (a persisted record variable is a constructor CALL and needs
    /// its type back first; a variable may use an imported name). `ans` is
    /// never touched.
    public func restoreSession(from workbook: Workbook) {
        environment.replaceUserFunctions([:])
        environment.replaceUserDataTypes([:])
        environment.clearImports()
        environment.clearNamespaceSources()
        environment.clearNamespaceVariables()
        for source in workbook.namespaces {
            _ = evaluate(source) // re-registers the namespace's members; re-records the source
        }
        for namespace in workbook.imports {
            _ = evaluate("import \(namespace)")
        }
        for source in workbook.dataTypes.values.sorted() {
            _ = evaluate(source) // bad hand-edited lines are dropped
        }
        for source in workbook.functions.sorted() {
            _ = evaluate(source)
        }
        restoreVariables(workbook.variables)
    }
}
