Feature: Scripts — multi-line sources split into logical statements
  As a user running .anzan files (or pasting multi-line programs)
  I want statements to continue across lines inside open brackets
  So that pretty-formatted programs run exactly like their one-line forms

  # The engine primitive both CLIs (and later the apps) share: a statement ends
  # at a newline UNLESS a ( [ { is still open — then lines join into ONE
  # logical line, so carets, echoes, and docs behave exactly as if it had been
  # typed on one line. `the result is` asserts the LAST statement's outcome.
  Scenario: A pretty multi-line namespace block runs as one statement
    When I run the script:
      """
      namespace Cash {
          data Change { nickels: Number };
          coins(c, d) = if(c < d, 0, 1 + coins(c - d, d))
      }
      Cash::coins(15, 5)
      """
    Then the result is "3"

  Scenario: Parentheses continue a statement across lines
    When I run the script:
      """
      sum(
          1, 2,
          3, 4
      )
      """
    Then the result is "10"

  Scenario: Brackets continue a statement across lines
    When I run the script:
      """
      len([
          1, 2,
          3
      ])
      """
    Then the result is "3"

  # `#!` is an ordinary `#` comment, so a shebang line is a note — .anzan files
  # are directly executable (chmod +x + `#!/usr/bin/env soroban`).
  Scenario: Shebang and comment lines pass between statements
    When I run the script:
      """
      #!/usr/bin/env soroban
      # a note to the reader
      1 + 1
      """
    Then the result is "2"

  # The FIRST physical line's trailing comment survives the join, so a
  # multi-line definition still documents itself the way one-liners do.
  Scenario: A first-line comment documents a multi-line definition
    When I run the script:
      """
      triple(x) = (    # three of x
          x * 3
      )
      """
    When I calculate "man triple"
    Then documentation is shown mentioning "three of x"

  # Brackets inside string literals are text, not structure.
  Scenario: Braces inside a string do not open a continuation
    When I run the script:
      """
      s = "{ not a block"
      len(s)
      """
    Then the result is "13"

  Scenario: An unclosed block at end of input is a loud error
    When I run the script:
      """
      namespace Broken {
          x() = 1
      """
    Then the calculation fails mentioning "unterminated"

  # Modes compose with scripts: finance-mode grouping works inside a
  # continued (non-call) paren, where the `,` can't be an argument separator.
  Scenario: Finance-mode grouping works inside a continued statement
    Given the calculator is in finance mode
    When I run the script:
      """
      x = (
          138,561 * 9%
      )
      x
      """
    Then the result is "12470.49"
    And the log echoes "12,470.49"
