Feature: Working in the grid
  As a spreadsheet user
  I want cells, names, definitions, and controls to cooperate
  So that my model reads like what I mean

  Scenario: Cells reference each other
    Given cell B:1 contains "1200"
    And cell B:2 contains "=B:1 * 2"
    Then cell B:2 shows "2400"

  Scenario: Labels never become errors
    Given cell A:1 contains "Q1 revenue"
    Then cell A:1 shows "Q1 revenue"

  # A comment-only cell is a free-floating note: it shows its text, holds no
  # value, and is skipped in ranges (like text). A trailing comment on a
  # formula cell is kept in the raw and doesn't affect the result.
  Scenario: A comment cell is a note that holds no value
    Given cell A:1 contains "# tentative — revisit in Q4"
    Then cell A:1 shows "# tentative — revisit in Q4"

  Scenario: A note cell is skipped in ranges and errors when referenced
    Given the sheet contains:
      | cell | value |
      | B:1  | 100   |
      | B:2  | # estimate |
      | B:3  | 50    |
    And cell B:4 contains "=sum(B:1..B:3)"
    Then cell B:4 shows "150"
    And cell B:5 contains "=B:2 + 1"
    Then cell B:5 shows an error mentioning "not a number"

  Scenario: A trailing comment on a formula is kept and ignored by evaluation
    Given cell B:1 contains "200"
    And cell B:2 contains "=B:1 * 1.08 # with sales tax"
    Then cell B:2 shows "216"

  Scenario: Ranges aggregate a column
    Given the sheet contains:
      | cell | value |
      | B:1  | 100   |
      | B:2  | 250.5 |
      | B:3  | 49.5  |
    And cell B:4 contains "=sum(B:1..B:3)"
    Then cell B:4 shows "400"

  Scenario: A named cell reads like prose
    Given cell B:7 contains "0.08"
    And cell B:7 is named "Projected Rate"
    And cell A:1 contains "='Projected Rate' * 100"
    Then cell A:1 shows "8"

  Scenario: Sheet definitions belong to their cells
    Given cell A:1 contains "rate = 0.1"
    And cell A:2 contains "=100 * rate"
    Then cell A:1 shows "𝑖 rate"
    And cell A:2 shows "10"
    When I calculate "rate = 0.5"
    Then the calculation fails mentioning "defined in cell"

  Scenario: A function defined in a cell is callable
    Given cell A:1 contains "tax(x) = x * 1.0825"
    And cell A:2 contains "=tax(200)"
    Then cell A:1 shows "λ tax(x)"
    And cell A:2 shows "216.5"

  Scenario: A slider is a value with a range
    Given cell A:1 contains "r = slider(0.11, 0, 0.2)"
    And cell A:2 contains "=r * 100"
    Then cell A:1 is a slider set to "0.11"
    And cell A:2 shows "11"

  Scenario: The log sees the sheet
    Given cell B:1 contains "1200"
    When I calculate "B:1 * 2"
    Then the result is "2400"

  Scenario: Explicit markers override auto-detection
    Given cell A:1 contains ""123""
    And cell A:2 contains "=12 * rte"
    Then cell A:1 shows "123"
    And cell A:2 shows an error mentioning "unknown variable"

  Scenario: A formula mistake is an error, not a guess
    Given cell B:1 contains "100"
    And cell B:2 contains "B:1 / 0"
    Then cell B:2 shows an error mentioning "division by zero"

  Scenario: Empty cells read as zero
    Given cell A:1 contains "=B:9 + 5"
    Then cell A:1 shows "5"

  Scenario: Circular references are caught, not infinite
    Given cell A:1 contains "=A:2 + 1"
    And cell A:2 contains "=A:1 + 1"
    Then cell A:1 shows an error mentioning "circular reference"

  Scenario: Referencing a text cell is an error
    Given cell A:1 contains "Q1 revenue"
    And cell A:2 contains "=A:1 * 2"
    Then cell A:2 shows an error mentioning "not a number"

  Scenario: The controls family holds values
    Given cell A:1 contains "flag = checkbox(true)"
    And cell A:2 contains "n = stepper(5, 1, 20)"
    And cell A:3 contains "region = dropdown("EU", ["EU", "US"])"
    And cell B:1 contains "=if(flag, n * 2, 0)"
    And cell B:2 contains "=if(region == "EU", 1, 2)"
    Then cell B:1 shows "10"
    And cell B:2 shows "1"

  Scenario: A column of checkboxes counts its checked ones
    Given the sheet contains:
      | cell | value           |
      | C:1  | checkbox(true)  |
      | C:2  | checkbox(false) |
      | C:3  | checkbox(true)  |
    And cell C:4 contains "=sum(C:1..C:3)"
    Then cell C:4 shows "2"

  Scenario: Formulas reach across worksheets by name
    Given a sheet named "Budget"
    And cell B:1 on "Budget" contains "1200"
    And cell A:1 contains "=Budget!B:1 * 2"
    Then cell A:1 shows "2400"
    When I calculate "sum(Budget!B:1..B:1) + 1"
    Then the result is "1201"

  Scenario: A renamed-away sheet breaks loudly, not silently
    Given cell A:1 contains "=Nowhere!B:1"
    Then cell A:1 shows an error mentioning "unknown sheet"

  Scenario: A workbook round-trips through its file format
    Given a sheet named "Loan"
    And cell B:1 on "Loan" contains "350000"
    And cell A:1 contains "=Loan!B:1 / 100"
    And cell B:7 contains "0.08"
    And cell B:7 is named "Projected Rate"
    And cell A:2 contains "='Projected Rate' * 100"
    When I calculate "growth = 1.5"
    And the workbook is saved and reopened
    Then cell A:1 shows "3500"
    And cell A:2 shows "8"
    When I calculate "growth * 2"
    Then the result is "3"
