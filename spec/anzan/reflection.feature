Feature: Inspecting the workbook
  As someone modelling in the grid
  I want to read the workbook's own structure from a formula
  So that a calculation can adapt to its sheets and cells

  # The reflection API is READ-ONLY: `Workbook` and the flat `cell()` /
  # `sheetNames()` accessors inspect the workbook but never change it. Cell
  # reads through it are LIVE — a formula that reads a cell this way recomputes
  # when that cell changes, exactly like a plain A:1 reference. Inner quotes are
  # bare (the Anzan string boundary), matching the rest of the suite.

  Scenario: Workbook reports its sheets
    Given a sheet named "Budget"
    When I calculate "Workbook.count"
    Then the result is "2"

  Scenario: Worksheet names are readable
    Given a sheet named "Budget"
    When I calculate "Workbook.worksheets[1].name == "Budget""
    Then the result is "1"

  Scenario: A worksheet is reachable by name
    Given a sheet named "Budget"
    When I calculate "Workbook.worksheets["Budget"].name == "Budget""
    Then the result is "1"

  Scenario: A negative index counts from the end
    Given a sheet named "Budget"
    When I calculate "Workbook.worksheets[-1].name == "Budget""
    Then the result is "1"

  Scenario: Reading a cell's value through the object graph
    Given a sheet named "Budget"
    And cell B:1 on "Budget" contains "1200"
    When I calculate "Workbook.worksheets["Budget"].cell("B", 1).value * 2"
    Then the result is "2400"

  Scenario: A cell value is reachable by position
    Given cell A:1 contains "=2 + 3"
    When I calculate "Workbook.worksheets[0].cell("A", 1).value"
    Then the result is "5"

  Scenario: The flat cell() accessor reads the active sheet
    Given cell A:1 contains "42"
    When I calculate "cell("A", 1).value"
    Then the result is "42"

  Scenario: The flat cell() accessor reaches another sheet by name
    Given a sheet named "Budget"
    And cell A:1 on "Budget" contains "99"
    When I calculate "cell("Budget", "A", 1).value"
    Then the result is "99"

  Scenario: sheetNames() lists every sheet
    Given a sheet named "Budget"
    When I calculate "len(sheetNames())"
    Then the result is "2"

  Scenario: rowCount() reports the grid size
    When I calculate "rowCount()"
    Then the result is "1000"

  Scenario: A formula reading a cell through reflection recomputes live
    Given cell A:1 contains "10"
    And cell B:1 contains "=cell("A", 1).value + 1"
    Then cell B:1 shows "11"

  Scenario: A user's own function shadows a reflection accessor
    When I calculate "cell(x) = x * 10"
    And I calculate "cell(5)"
    Then the result is "50"

  Scenario: An unknown sheet is reported clearly
    When I calculate "cell("Nope", "A", 1).value"
    Then the calculation fails mentioning "unknown sheet"
