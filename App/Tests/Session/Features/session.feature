Feature: The session: edits, undo, and live rewrites
  As a user working in the grid
  I want every change reversible and every rename honest
  So that I can explore without fear

  Scenario: Undo and redo walk a cell edit
    Given cell A:1 contains "100"
    Then cell A:1 shows "100"
    When I undo
    Then the contents of cell A:1 are ""
    When I redo
    Then cell A:1 shows "100"

  Scenario: Renaming a named cell rewrites every referencing formula
    Given cell B:7 contains "0.08"
    And cell B:7 is named "Rate"
    And cell A:1 contains "='Rate' * 100"
    When cell B:7 is renamed to "APR"
    Then the contents of cell A:1 are "='APR' * 100"
    And cell A:1 shows "8"

  Scenario: Removing a name can inline the address instead of breaking formulas
    Given cell B:7 contains "0.08"
    And cell B:7 is named "Rate"
    And cell A:1 contains "='Rate' * 100"
    When the name of cell B:7 is removed, replacing references with its address
    Then the contents of cell A:1 are "=B:7 * 100"
    And cell A:1 shows "8"

  Scenario: Undo unwinds a rename completely — formulas AND the name
    Given cell B:7 contains "0.08"
    And cell B:7 is named "Rate"
    And cell A:1 contains "='Rate' * 100"
    When cell B:7 is renamed to "APR"
    And I undo
    And I undo
    Then the contents of cell A:1 are "='Rate' * 100"
    And cell A:1 shows "8"

  # History reflection: the calculation log as a queryable array (log-only).
  Scenario: History reflects the calculation log
    Given the log has run "100"
    And the log has run "ans + 50"
    And the log has run "8%"
    Then evaluating "len(History)" gives "3"
    And evaluating "History[0].value" gives "100"
    And evaluating "History[1].value" gives "150"
    And evaluating "last(History).value" gives "0.08"
    And evaluating "History[2].input" gives "8%"
    And evaluating "History[2].kind" gives "value"
    And evaluating "sum(map(entry -> entry.value, History))" gives "250.08"

  Scenario: History entries classify and trace their lines
    Given the log has run "1 / 0"
    And the log has run "# a note"
    And the log has run "A:1 + 10"
    Then evaluating "History[0].kind" gives "error"
    And evaluating "History[0].isError" gives "1"
    And evaluating "History[1].kind" gives "comment"
    And evaluating "History[2].referencesCells" gives "1"

  # In a CELL, History is just a text label (not an error) — the log is session
  # state, so a cell can't read it; a header literally named "History" is fine.
  Scenario: History is a plain text label inside a cell
    Given cell A:1 contains "History"
    Then cell A:1 shows "History"

  # Dumping bare History yields an array of reflection handles whose rendering
  # isn't re-parseable — so it's logged display-only ("info"), not a recallable
  # value. (len(History) is the clean way to get the size.)
  Scenario: A History dump is recorded display-only, not as a value
    Given the log has run "100"
    And the log has run "History"
    Then evaluating "History[0].kind" gives "value"
    And evaluating "History[1].kind" gives "info"
    And evaluating "len(History)" gives "2"

  Scenario: Renaming a sheet rewrites referencing formulas everywhere
    Given a new sheet named "Budget" is added
    And cell B:1 contains "250"
    When sheet "Sheet 1" is activated
    Given cell A:1 contains "=Budget!B:1 * 2"
    And cell A:2 contains "='Budget'!B:1 + 50"
    When sheet "Budget" is activated
    And the active sheet is renamed to "Plan"
    And sheet "Sheet 1" is activated
    Then the contents of cell A:1 are "=Plan!B:1 * 2"
    And the contents of cell A:2 are "=Plan!B:1 + 50"
    And cell A:1 shows "500"
    And cell A:2 shows "300"

  Scenario: Undo unwinds a sheet rename completely — formulas AND the name
    Given a new sheet named "Budget" is added
    And cell B:1 contains "250"
    When sheet "Sheet 1" is activated
    Given cell A:1 contains "='Budget'!B:1 + 50"
    When sheet "Budget" is activated
    And the active sheet is renamed to "Plan"
    And I undo
    And I undo
    Then the active sheet is named "Budget"
    When sheet "Sheet 1" is activated
    Then the contents of cell A:1 are "='Budget'!B:1 + 50"
    And cell A:1 shows "300"

  Scenario: Fill down adjusts relative references and holds pins
    Given cell A:1 contains "10"
    And cell A:2 contains "20"
    And cell A:3 contains "30"
    And cell C:1 contains "2"
    And cell B:1 contains "=A:1 * $C:$1"
    And cells B:1 through B:3 are selected
    When I fill down
    Then the contents of cell B:2 are "=A:2 * $C:$1"
    And the contents of cell B:3 are "=A:3 * $C:$1"
    And cell B:3 shows "60"
    When I undo
    Then the contents of cell B:2 are ""

  Scenario: Fill right walks columns and a single cell fills from its neighbor
    Given cell A:1 contains "5"
    And cell A:2 contains "=A:1 * 10"
    And cells B:2 through B:2 are selected
    When I fill right
    Then the contents of cell B:2 are "=B:1 * 10"

  Scenario: Pasting an in-app copy adjusts references
    Given cell A:1 contains "5"
    And cell A:2 contains "=A:1 + 1"
    And cells A:2 through A:2 are selected
    When the selection is copied
    And cell C:5 is selected
    And the copied cells are pasted
    Then the contents of cell C:5 are "=C:4 + 1"

  Scenario: External text pastes verbatim
    Given cell A:1 contains "5"
    When cell D:1 is selected
    And the text "=A:1 + 1" is pasted from outside
    Then the contents of cell D:1 are "=A:1 + 1"
    And cell D:1 shows "6"

  Scenario: Pasting above the source turns dead references into refError
    Given cell B:2 contains "=B:1 + 1"
    And cells B:2 through B:2 are selected
    When the selection is copied
    And cell C:1 is selected
    And the copied cells are pasted
    Then the contents of cell C:1 are "=refError() + 1"

  Scenario: Inserting rows shifts content and formulas together
    Given cell A:2 contains "100"
    And cell B:5 contains "=A:2 * 2"
    When 2 rows are inserted above row 1
    Then the contents of cell A:4 are "100"
    And the contents of cell B:7 are "=A:4 * 2"
    And cell B:7 shows "200"

  Scenario: Deleting a row breaks its readers loudly and shrinks ranges
    Given cell A:1 contains "10"
    And cell A:2 contains "20"
    And cell A:3 contains "30"
    And cell B:1 contains "=A:2 * 2"
    And cell C:1 contains "=sum(A:1..A:3)"
    When row 2 is deleted
    Then the contents of cell B:1 are "=refError() * 2"
    And the contents of cell C:1 are "=sum(A:1..A:2)"
    And cell C:1 shows "40"

  Scenario: Undo restores a deleted column exactly
    Given cell B:1 contains "0.08"
    And cell C:1 contains "=B:1 * 100"
    When column B is deleted
    Then the contents of cell B:1 are "=refError() * 100"
    When I undo
    Then the contents of cell B:1 are "0.08"
    And the contents of cell C:1 are "=B:1 * 100"
    And cell C:1 shows "8"
    When I redo
    Then the contents of cell B:1 are "=refError() * 100"

  Scenario: A comment cell is a note that survives a round trip
    Given cell A:1 contains "# revisit these assumptions in Q4"
    Then cell A:1 shows "# revisit these assumptions in Q4"
    And the contents of cell A:1 are "# revisit these assumptions in Q4"

  Scenario: A trailing comment is kept on a formula cell
    Given cell B:1 contains "200"
    And cell B:2 contains "=B:1 * 1.08 # with sales tax"
    Then cell B:2 shows "216"
    And the contents of cell B:2 are "=B:1 * 1.08 # with sales tax"

  Scenario: Clicking a checkbox rewrites its own cell text
    Given cell A:1 contains "flag = checkbox(true)"
    When the control in cell A:1 commits "false"
    Then the contents of cell A:1 are "flag = checkbox(false)"

  Scenario: Releasing a slider writes one undoable edit
    Given cell A:1 contains "r = slider(0.08, 0, 0.2)"
    And cell A:2 contains "=r * 100"
    When the slider in cell A:1 is released at the top
    Then the contents of cell A:1 are "r = slider(0.2, 0, 0.2)"
    And cell A:2 shows "20"
    When I undo
    Then the contents of cell A:1 are "r = slider(0.08, 0, 0.2)"
    And cell A:2 shows "8"

  Scenario: Exporting the sheet writes computed values, not formulas
    Given cell A:1 contains "Subtotal"
    And cell B:1 contains "100"
    And cell B:2 contains "=B:1 * 2"
    Then the CSV export contains the line "Subtotal,100"
    And the CSV export contains the line ",200"

  # Workbook-mutation commands driven from the log route through the same
  # undo/persistence machinery as UI edits. Inner quotes are bare.
  Scenario: A log updateCell command writes one undoable edit
    Given cell A:1 contains "10"
    When I run "updateCell(cell("A", 1), 99)" in the log
    Then cell A:1 shows "99"
    When I undo
    Then cell A:1 shows "10"

  Scenario: A log command adds a worksheet
    When I run "addWorksheet("Budget")" in the log
    Then the active sheet is named "Budget"

  Scenario: A log command renames a worksheet
    Given a new sheet named "Budget" is added
    When I run "renameWorksheet("Budget", "Costs")" in the log
    Then the active sheet is named "Costs"
