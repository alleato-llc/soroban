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

  # docs/MODES.md — Finance keeps the Normal arithmetic core: ^ is power, % is
  # percent, and the Programmer glyphs still error. It ADDS two literal forms
  # (currency and grouped numbers); everything else reads as Normal does.
  Scenario Outline: Finance mode keeps the Normal arithmetic core
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

  # docs/MODES.md — currency is a first-class tagged TYPE (a peer of Int32/Decimal).
  # `the result is` checks the CANONICAL form — the mode-agnostic constructor
  # Money(v, "CODE"), what persists and recalls — while `the log echoes` checks
  # the human display, $10.00 (grouped, 2 decimals, symbol outside the sign).
  # The currency PROPAGATES through arithmetic, so a money input yields money.
  Scenario Outline: Finance mode carries currency through arithmetic
    Given the calculator is in finance mode
    When I calculate "<expr>"
    Then the result is "<canonical>"
    And the log echoes "<echo>"

    Examples:
      | expr        | canonical              | echo       |
      | $10         | Money(10, "USD")       | $10.00     |
      | $10 * 5%    | Money(0.5, "USD")      | $0.50      |
      | €10 * .4    | Money(4, "EUR")        | €4.00      |
      | $10 + $5    | Money(15, "USD")       | $15.00     |
      | $10 - $2.50 | Money(7.5, "USD")      | $7.50      |
      | $100 / 4    | Money(25, "USD")       | $25.00     |
      | £1234.567   | Money(1234.567, "GBP") | £1,234.57  |
      | ¥5000 * 2   | Money(10000, "JPY")    | ¥10,000.00 |

  # The constructor is mode-agnostic — it works in Normal mode too (the $-literal
  # does not). This is the persistence/recall form: it re-parses by evaluation in
  # any mode, exactly like Decimal(…) / Int32(…).
  Scenario: The Money constructor works in normal mode
    When I calculate "Money(10, "USD")"
    Then the result is "Money(10, "USD")"
    And the log echoes "$10.00"

  Scenario: An unknown currency code is refused
    When I calculate "Money(10, "XYZ")"
    Then the calculation fails mentioning "unknown currency"

  # The currency set is closed — an unsupported currency glyph is a loud lex
  # error, not a silent pass. (Use the constructor for currencies without a glyph.)
  Scenario: An unsupported currency glyph is refused
    Given the calculator is in finance mode
    When I calculate "₫100"
    Then the calculation fails mentioning "unsupported currency"

  # `%` scales a plain number, so applying it to a currency amount is a category
  # error ("$9 as a percent" is meaningless). It's refused loudly rather than
  # silently — the symbol doesn't change the number, so $9% and 9% would
  # otherwise be indistinguishable. Note `$10 * 5%` is fine: there the % is on
  # the plain 5, never on the money.
  Scenario: Percent on a currency amount is refused
    Given the calculator is in finance mode
    When I calculate "$9%"
    Then the calculation fails mentioning "can't apply % to a currency amount"

  # A plain number is ABSORBED by the currency operand — you don't have to spell
  # the symbol on both sides. (Percent relies on this: `5%` evaluates to a plain
  # 0.05 before it ever reaches the multiply.)
  Scenario Outline: A plain number absorbs into the currency
    Given the calculator is in finance mode
    When I calculate "<expr>"
    Then the log echoes "<echo>"

    Examples:
      | expr    | echo   |
      | $10 + 5 | $15.00 |
      | 5 + $10 | $15.00 |
      | $10 * 3 | $30.00 |

  # Two different currencies is a hard error — there is no exchange rate to
  # apply, so guessing would be worse than refusing.
  Scenario Outline: Mixing currencies is refused
    Given the calculator is in finance mode
    When I calculate "<expr>"
    Then the calculation fails mentioning "can't mix currencies"

    Examples:
      | expr     |
      | $10 + €5 |
      | €10 - $5 |
      | $10 * £2 |

  # The tag survives ALL FOUR operators, so the format stays consistent — a
  # money input always reads back as money. This deliberately does not model
  # dimensionality ($10 * $2 is $20.00, not "dollars squared"): the tag is a
  # display contract, not a unit system.
  Scenario Outline: The currency survives every arithmetic operator
    Given the calculator is in finance mode
    When I calculate "<expr>"
    Then the log echoes "<echo>"

    Examples:
      | expr     | echo   |
      | $10 * $2 | $20.00 |
      | $10 / $2 | $5.00  |
      | $10 + $2 | $12.00 |
      | $10 - $2 | $8.00  |

  # Negating money keeps it money, and the sign sits OUTSIDE the symbol —
  # matching how the sheet's currency format already renders negatives.
  Scenario: Negation keeps the currency and puts the sign outside the symbol
    Given the calculator is in finance mode
    When I calculate "-$1234.5"
    Then the result is "Money(-1234.5, "USD")"
    And the log echoes "-$1,234.50"

  # Currency literals are a FINANCE dialect — in Normal mode `$` still means the
  # cell-reference column pin, and that error must not regress into silence.
  Scenario: Currency literals are finance-mode only
    When I calculate "$10 + $5"
    Then the calculation fails mentioning "'$' pins a cell reference"

  # `,` groups the thousands of a numeric literal: 1-3 digits, then any number of
  # exactly-3-digit groups. Grouping is PRESENTATION — the canonical form is the
  # plain number, but it ECHOES, so an input that grouped gets a grouped answer
  # back (at the value's own decimals — padding is money's rule, not grouping's).
  Scenario Outline: Finance mode reads grouped numbers and echoes the grouping
    Given the calculator is in finance mode
    When I calculate "<expr>"
    Then the result is "<canonical>"
    And the log echoes "<echo>"

    Examples:
      | expr          | canonical | echo      |
      | 138,561       | 138561    | 138,561   |
      | 138,561 * 9%  | 12470.49  | 12,470.49 |
      | 1,000 + 1     | 1001      | 1,001     |
      | 1,234,567     | 1234567   | 1,234,567 |
      | 12,470.49 * 2 | 24940.98  | 24,940.98 |
      | -138,561      | -138561   | -138,561  |

  # Grouping composes with currency — the money literal is the same number
  # grammar behind the symbol.
  Scenario: Grouped currency composes through a subexpression
    Given the calculator is in finance mode
    When I calculate "$10,000 + ($15,000 * 5%)"
    Then the result is "Money(10750, "USD")"
    And the log echoes "$10,750.00"

  # `,` is the argument separator FIRST. Grouping is suppressed inside a call's
  # argument list (and inside [ ] / { } literals), so existing code cannot change
  # meaning under finance mode.
  Scenario: The argument separator wins over grouping
    Given the calculator is in finance mode
    When I calculate "max(138,561)"
    Then the result is "561"

  Scenario: Grouping is suppressed inside an array literal
    Given the calculator is in finance mode
    When I calculate "sum([1,500])"
    Then the result is "501"

  # A malformed group is a loud lex error, not a silent two-number misparse.
  Scenario Outline: Malformed thousands groups are refused
    Given the calculator is in finance mode
    When I calculate "<expr>"
    Then the calculation fails mentioning "thousands group"

    Examples:
      | expr     |
      | 1,23     |
      | 1,2345   |
      | 1234,567 |

  # Grouped literals are a FINANCE dialect too — Normal mode still reads `,` as a
  # separator everywhere, so a bare grouped number is the usual "can't directly
  # follow" error rather than a number.
  Scenario: Grouped literals are finance-mode only
    When I calculate "138,561"
    Then the calculation fails mentioning "unexpected trailing input"
