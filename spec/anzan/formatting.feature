Feature: Number formats are presentation, never value
  As someone formatting a model for reading
  I want currency, percent, and date displays
  So that cells read naturally while the math stays exact

  Scenario Outline: A formatted cell displays its value in costume
    Given cell B:1 contains "<value>"
    And cell B:1 is formatted as "<format>"
    Then cell B:1 displays "<displayed>"

    Examples:
      | value      | format  | displayed     |
      | 1234567.5  | number  | 1,234,567.50  |
      | 1234.5     | dollars | $1,234.50     |
      | -1234.5    | dollars | -$1,234.50    |
      | -2         | euros   | -€2.00        |
      | 0.0825     | percent | 8.25%         |
      | 1          | percent | 100.00%       |
      | 20610      | a date  | 2026-06-06    |
      | 0          | a date  | 1970-01-01    |
      | 195        | hex     | 0xC3          |
      | -255       | hex     | -0xFF         |
      | 195        | binary  | 0b1100_0011   |
      | 5          | binary  | 0b101         |
      | 1.5        | hex     | 1.5           |

  Scenario: Formatting never touches the underlying math
    Given cell B:1 contains "0.0825"
    And cell B:1 is formatted as "percent"
    And cell B:2 contains "=B:1 * 200"
    Then cell B:1 displays "8.25%"
    And cell B:2 shows "16.5"

  # The honest "programmer mode": hex display is a FORMAT, never a semantics
  # switch — the stored value stays an exact decimal, references see the
  # number, and the workbook renders the same on every machine.
  Scenario: Hex format is display-only, like every format
    Given cell A:1 contains "=bitOr(0xC0, 0x03)"
    And cell A:1 is formatted as "hex"
    And cell A:2 contains "=A:1 + 1"
    Then cell A:1 displays "0xC3"
    And cell A:2 shows "196"
