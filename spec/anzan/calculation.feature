Feature: Calculating in the log
  As someone doing money math
  I want exact decimal answers from typed expressions
  So that floating-point drift never lies to me

  Scenario Outline: Everyday calculations are exact
    When I calculate "<expression>"
    Then the result is "<result>"

    Examples:
      | expression                       | result |
      | 0.1 + 0.2                        | 0.3    |
      | 2(3 + 4)                         | 14     |
      | 2^10                             | 1024   |
      | ∑(1, 2, 3)                       | 6      |
      | ∑_i=1^10(i^2)                    | 385    |
      | pmt(0.05/12, 360, 200000)        | -1073.643246024277969656985158225109053609679713701 |
      | margin(100, 80)                  | 20     |
      | date(2026, 6, 6) - date(2026, 1, 1) | 156 |
      | if(2 > 1, 10, 20)                | 10     |
      | gcd(48, 36)                      | 12     |

  Scenario: A hundred dimes make exactly a dollar bag
    When I calculate "∑_i=1^100(0.1)"
    Then the result is "10"

  Scenario: The previous answer carries forward
    When I calculate "6 * 7"
    And I calculate "ans + 8"
    Then the result is "50"

  Scenario: Variables persist across calculations
    When I calculate "rate2026 = 0.0825"
    And I calculate "1200 * rate2026"
    Then the result is "99"

  Scenario: A leading equals sign is tolerated
    When I calculate "= 1 + 2"
    Then the result is "3"

  Scenario: A failed calculation never clobbers the previous answer
    When I calculate "6 * 7"
    And I calculate "1 / 0"
    And I calculate "ans + 0"
    Then the result is "42"

  Scenario: Dividing by zero explains itself
    When I calculate "1 / 0"
    Then the calculation fails mentioning "division by zero"

  Scenario: Typos are caught, not guessed
    When I calculate "12 * rte"
    Then the calculation fails mentioning "unknown variable 'rte'"

  Scenario Outline: Comparisons answer 1 or 0
    When I calculate "<expression>"
    Then the result is "<result>"

    Examples:
      | expression  | result |
      | 2 < 3       | 1      |
      | 3 < 2       | 0      |
      | 3 > 2       | 1      |
      | 2 <= 2      | 1      |
      | 2 >= 3      | 0      |
      | 2 == 2      | 1      |
      | 2 != 2      | 0      |

  Scenario: Comparison chains are rejected, not misread
    When I calculate "1 < 2 < 3"
    Then the calculation fails mentioning "can't be chained"
