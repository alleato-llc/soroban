import Anzan
/// The workbook half of session restore. Lives on the HOSTING side of the
/// module boundary: Calculator (Anzan) knows nothing about Workbook — this
/// extension is the one place the two meet.
extension Calculator {
    /// Replaces the session's definitions and variables from a workbook.
    /// Order matters: data types first, then functions, then variables —
    /// a persisted record variable is a constructor CALL (`Person(name: …)`)
    /// and needs its type defined to come back. `ans` is never touched.
    public func restoreSession(from workbook: Workbook) {
        environment.replaceUserFunctions([:])
        environment.replaceUserDataTypes([:])
        for source in workbook.dataTypes.values.sorted() {
            _ = evaluate(source) // bad hand-edited lines are dropped
        }
        for source in workbook.functions.sorted() {
            _ = evaluate(source)
        }
        restoreVariables(workbook.variables)
    }
}
