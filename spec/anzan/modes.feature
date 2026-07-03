Feature: Input/display modes are dialects over one canonical language
  As a reader of docs/MODES.md
  I want each mode's glyphs pinned to their meaning
  So that switching modes never changes what a stored formula means

  # docs/MODES.md — Programmer mode is a LOG dialect: the overloaded glyphs read
  # as bitwise / modulo (the canonical stored form is the function — bitXor, mod,
  # …); cells stay canonical. `^` is XOR here, so power is written pow().
  Scenario Outline: Programmer mode reads glyphs as bitwise and modulo
    Given the calculator is in programmer mode
    When I calculate "<expr>"
    Then the result is "<result>"

    Examples:
      | expr       | result |
      | 5 ^ 3      | 6      |
      | 12 & 10    | 8      |
      | 1 << 4     | 16     |
      | 8 >> 2     | 2      |
      | 17 % 5     | 2      |
      | pow(2, 10) | 1024   |

  # `|` is the table delimiter, so the OR case stands alone.
  Scenario: Programmer mode reads pipe as bitwise OR
    Given the calculator is in programmer mode
    When I calculate "12 | 3"
    Then the result is "15"

  # Python precedence: bitwise binds below arithmetic, above comparison (no
  # C-style `a & b == c` trap).
  Scenario: Programmer-mode bitwise precedence follows Python
    Given the calculator is in programmer mode
    When I calculate "1 & 1 == 1"
    Then the result is "1"

  # docs/MODES.md — the bitwise-only glyphs are Programmer-mode operators; in the
  # default (Normal) dialect they're a loud, mode-scoped error, not a misparse.
  Scenario Outline: Bitwise glyphs are Programmer-mode only
    When I calculate "<expr>"
    Then the calculation fails mentioning "Programmer-mode operator"

    Examples:
      | expr   |
      | 5 & 3  |
      | 1 << 2 |
      | 8 >> 1 |

  # docs/MODES.md — Finance is grammatically IDENTICAL to Normal today (it's a
  # home for future finance DISPLAY defaults). Pin that, so a later divergence
  # trips a test: ^ is power, % is percent, and the Programmer glyphs still error.
  Scenario Outline: Finance mode matches the Normal grammar
    Given the calculator is in finance mode
    When I calculate "<expr>"
    Then the result is "<result>"

    Examples:
      | expr  | result |
      | 2 ^ 3 | 8      |
      | 3%    | 0.03   |
      | 50%   | 0.5    |

  Scenario: Finance mode still rejects the Programmer-only glyphs
    Given the calculator is in finance mode
    When I calculate "5 & 3"
    Then the calculation fails mentioning "Programmer-mode operator"
