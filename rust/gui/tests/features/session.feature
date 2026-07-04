Feature: The Rust app session — calculator and sheet, headless

  Driven directly against the UI-free `Session` (no iced, no rendering) — the
  Rust counterpart to the Swift SorobanSessionTests, but a fast `cargo test`.
  This suite is Rust-only (it exercises what the port supports today); the
  cross-ecosystem parity oracle is spec/anzan, run by the engine's gherkin suite.

  Background:
    Given a fresh session

  # ---- Calculator (the log) ----

  Scenario: A calculation lands in the log at full precision
    When I enter "0.1 + 0.2"
    Then the result is "0.3"

  Scenario: ans carries the last result across lines
    When I enter "6 * 7"
    And I enter "ans + 1"
    Then the result is "43"

  Scenario: Defining a function is reported, then it is callable
    When I enter "double(x) = x * 2"
    Then the log defines a function "double(x)"
    When I enter "double(21)"
    Then the result is "42"

  Scenario: A bad expression fails with a message
    When I enter "1 / 0"
    Then the last line fails mentioning "division by zero"

  Scenario: A comment line is recorded as a note
    When I enter "# just a note"
    Then the last line is a note "just a note"

  # ---- Sheet (the grid) ----

  Scenario: A number cell shows its value
    When I set cell A:1 to "1200"
    Then cell A:1 shows "1200"

  Scenario: A formula reads another cell
    When I set cell A:1 to "1200"
    And I set cell A:2 to "=A:1 * 2"
    Then cell A:2 shows "2400"

  Scenario: A label stays text; a bad formula shows an error
    When I set cell A:1 to "Revenue"
    Then cell A:1 shows "Revenue"
    When I set cell B:1 to "=1 / 0"
    Then cell B:1 shows an error mentioning "division by zero"

  Scenario: The log and the grid share one variable space
    When I enter "rate = 0.1"
    And I set cell A:1 to "=100 * rate"
    Then cell A:1 shows "10"

  # ---- Undo / redo ----

  Scenario: Undo and redo walk a cell edit
    When I set cell A:1 to "5"
    And I set cell A:1 to "9"
    Then cell A:1 shows "9"
    When I undo
    Then cell A:1 shows "5"
    When I redo
    Then cell A:1 shows "9"

  # ---- Controls ----

  Scenario: Toggling a checkbox rewrites its own cell
    When I set cell A:1 to "flag = checkbox(true)"
    Then cell A:1 shows "checked"
    When I toggle the checkbox in A:1
    Then cell A:1 shows "unchecked"

  Scenario: A slider commits its value into the cell text
    When I set cell A:1 to "rate = slider(0.08, 0, 0.2)"
    And I set the slider in A:1 to "0.12"
    Then cell A:1 shows "slider:0.12"

  # ---- Copy / paste (TSV) ----

  Scenario: Copy then paste moves cell contents
    When I set cell A:1 to "5"
    And I set cell A:2 to "6"
    And I copy A:1 through A:2
    And I paste at C:1
    Then cell C:1 shows "5"
    And cell C:2 shows "6"

  # ---- Named cells ----

  Scenario: A named cell reads by name from a formula
    When I set cell A:1 to "0.0825"
    And I name cell A:1 "Rate"
    And I set cell B:1 to "=100 * 'Rate'"
    Then cell B:1 shows "8.25"

  # ---- Column widths ----

  Scenario: A column width round-trips
    When I set column A width to "150"
    Then column A width is "150"

  # ---- Workbook round trip ----

  Scenario: Saving and reopening restores cells and variables
    When I enter "rate = 0.25"
    And I set cell A:1 to "=rate * 4"
    And I save and reopen the workbook
    Then cell A:1 shows "1"
