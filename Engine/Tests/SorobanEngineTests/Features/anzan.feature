Feature: The Anzan language itself
  As a reader of docs/ANZAN.md
  I want the grammar's promises to be executable
  So that the spec and the engine can never quietly disagree

  # docs/ANZAN.md §3 — every row pins one precedence/associativity rule.
  Scenario Outline: Operator precedence and associativity
    When I calculate "<expression>"
    Then the result is "<result>"

    Examples:
      | expression    | result | # rule                                   |
      | 2 + 3 * 4     | 14     | # multiplicative over additive           |
      | 2^3^2         | 512    | # ^ is right-associative                 |
      | -2^2          | -4     | # unary minus binds looser than ^        |
      | 2^-2          | 0.25   | # the exponent may carry its own sign    |
      | [10, 2][0]^2  | 100    | # postfix indexing binds tighter than ^  |
      | √4 + 5        | 7      | # prefix √ is unary, not greedy          |

  # docs/ANZAN.md §1 — the number lexicon.
  Scenario Outline: Number literals
    When I calculate "<expression>"
    Then the result is "<result>"

    Examples:
      | expression | result |
      | 1_000 + 0  | 1000   |
      | 2.5e-3     | 0.0025 |
      | .5 + .5    | 1      |
      | 0xFF       | 255    |
      | 0b1010     | 10     |
      | 0xDEAD_BEEF | 3735928559 |

  # docs/ANZAN.md §10 — $ pins are copy-time data for fill and paste; to the
  # evaluator $A:$1 and A:1 are the same cell.
  Scenario Outline: Pinned cell references evaluate like plain ones
    Given cell A:1 contains "21"
    When I calculate "<expression>"
    Then the result is "42"

    Examples:
      | expression |
      | $A:$1 * 2  |
      | $A:1 + A:1 |
      | A:$1 * 2   |

  Scenario Outline: A dangling $ is a loud lex error
    When I calculate "<expression>"
    Then the calculation fails mentioning "pins a cell reference"

    Examples:
      | expression |
      | $          |
      | $x         |
      | 2 + $rate  |

  # docs/ANZAN.md §1 — a line that is ONLY a comment is a first-class note,
  # not a parse error; a '#' inside a string stays literal.
  Scenario: A comment-only line is a note, not an error
    When I calculate "# the calc below confirms our test"
    Then the result is "# the calc below confirms our test"

  Scenario: A standalone note never touches ans
    When I calculate "21 * 2"
    And I calculate "# just thinking out loud"
    And I calculate "ans"
    Then the result is "42"

  Scenario: A trailing comment is stripped before evaluation
    When I calculate "5 + 3 # adds them"
    Then the result is "8"

  Scenario: A hash inside a string is not a comment
    When I calculate "len("a # b")"
    Then the result is "5"

  # docs/ANZAN.md §1 — a dangling character after a programmer literal is a
  # loud lex error, never a silent implicit multiplication.
  Scenario Outline: Malformed programmer literals fail loudly
    When I calculate "<expression>"
    Then the calculation fails mentioning "<message>"

    Examples:
      | expression | message       |
      | 0xFG       | malformed hex |
      | 0x1.5      | malformed hex |
      | 0x         | needs digits  |
      | 0b12       | malformed binary |

  # docs/ANZAN.md §3 — implicit multiplication's shapes.
  Scenario: Implicit multiplication covers parens, names, and constants
    When I calculate "x9 = 4"
    And I calculate "(2)(3) + 2x9"
    Then the result is "14"

  # docs/ANZAN.md §4 — modulo is the mod() function (the % operator is percent),
  # and it's exact decimal math (in binary floating point mod(0.3, 0.1) is
  # famously 0.0999…).
  Scenario: Modulo is exact
    When I calculate "mod(0.3, 0.1)"
    Then the result is "0"

  # docs/ANZAN.md §3 — postfix % is a percent literal: x% ≡ x × 0.01, exact,
  # binding tighter than ^ (so 1 * 3% is 1 * 0.03, not (1 * 3)%). Modulo moved
  # to mod() to free the symbol.
  Scenario Outline: Percent literals
    When I calculate "<expr>"
    Then the result is "<result>"

    Examples:
      | expr        | result |
      | 3%          | 0.03   |
      | 1 * 3%      | 0.03   |
      | 100 + 5%    | 100.05 |
      | 50% * 2     | 1      |
      | (2 + 3)%    | 0.05   |

  # docs/ANZAN.md §3 — implicit multiplication is a value against a NAME, paren,
  # or cell (2x, 2pi, 2(3+4), 2 A:1) — NOT against a bare number. Two numbers in
  # a row (3 4, or 3 % 4 = 3% then 4) is a missing operator, so it's an error,
  # not a silent ×. Modulo is mod(3, 4).
  Scenario Outline: A number can't directly follow another value
    When I calculate "<expr>"
    Then the calculation fails mentioning "operator"

    Examples:
      | expr   |
      | 3 4    |
      | 3 % 4  |
      | (1) 2  |

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

  # docs/FIXED-WIDTH.md — bounded, checked integers, both forms: parameterized
  # Int(value, bits) / UInt(value, bits) and per-width Int32(value).
  # The canonical (re-parseable) form is per-width. Arithmetic is exact AND
  # range-checked; width promotes to the widest; a plain integer literal adopts.
  Scenario Outline: Fixed-width construction and checked arithmetic
    When I calculate "<expr>"
    Then the result is "<result>"

    Examples:
      | expr                               | result        |
      | Int32(27374)                   | Int32(27374) |
      | Int(27374, 32)                 | Int32(27374) |
      | Int8(5) + Int8(3)          | Int8(8)   |
      | Int8(5) + 3                    | Int8(8)   |
      | Int(100, 8) + Int(100, 16) | Int16(200) |
      | Int8(2) ^ 3                    | Int8(8)   |

  # Checked, not modular: overflow is an ERROR, never a wraparound. Signs never
  # mix; a fractional value never silently truncates.
  Scenario Outline: Overflow and bad mixes are loud errors, never wraps
    When I calculate "<expr>"
    Then the calculation fails mentioning "<message>"

    Examples:
      | expr                          | message             |
      | Int8(200)                 | out of range        |
      | Int8(100) + Int8(100) | out of range        |
      | UInt8(0) - 1              | out of range        |
      | Int8(5) + UInt8(5)    | signed and unsigned |
      | Int8(5) + 1.5             | whole numbers       |

  # docs/FIXED-WIDTH.md — bitwise is two's-complement over the width and keeps the
  # type; ~ is bitwise NOT. (The OR case uses | — the table delimiter — so it's
  # exercised in the unit tests instead.)
  Scenario Outline: Programmer-mode bitwise preserves the fixed-width type
    Given the calculator is in programmer mode
    When I calculate "<expr>"
    Then the result is "<result>"

    Examples:
      | expr                          | result        |
      | UInt8(12) & UInt8(10) | UInt8(8)  |
      | ~UInt8(0)                 | UInt8(255) |
      | ~Int8(0)                  | Int8(-1)  |
      | UInt8(1) << 4             | UInt8(16) |

  # docs/DECIMAL.md — fixed-precision decimals: Decimal(value, precision, scale).
  # SQL DECIMAL(p,s) — rounds to scale (banker's by default), checked precision
  # (≤ 1000), padded to scale on display. Short forms Decimal(value) and
  # Decimal(value, scale) capture the value at max precision, which the canonical
  # form then HIDES (Decimal(0.5), Decimal(0.50, 2)). Arithmetic keeps the widest
  # scale/precision; a plain Number is absorbed and rounded to the decimal's scale.
  Scenario Outline: Fixed-precision decimal construction and arithmetic
    When I calculate "<expr>"
    Then the result is "<result>"

    Examples:
      | expr                                  | result                               |
      | Decimal(0.5)                          | Decimal(0.5)                         |
      | Decimal(3.14159)                      | Decimal(3.14159)                     |
      | Decimal(0.5, 2)                       | Decimal(0.50, 2)                     |
      | Decimal(10.5, 5, 2)                   | Decimal(10.50, 5, 2)                 |
      | Decimal(1.005, 5, 2)                  | Decimal(1.00, 5, 2)                  |
      | Decimal(1.005, 5, 2, Rounding.HalfUp) | Decimal(1.01, 5, 2, Rounding.HalfUp) |
      | Decimal(123, 5, 0)                    | Decimal(123, 5, 0)                   |
      | Decimal(2.50, 5, 2) + Decimal(1.25, 5, 2) | Decimal(3.75, 5, 2)              |
      | Decimal(10.00, 5, 2) - Decimal(0.01, 5, 2) | Decimal(9.99, 5, 2)             |
      | Decimal(2.50, 5, 2) * Decimal(4, 5, 2) | Decimal(10.00, 5, 2)                |
      | Decimal(10, 5, 2) / Decimal(3, 5, 2)  | Decimal(3.33, 5, 2)                  |
      | Decimal(10.00, 5, 2) + 0.005          | Decimal(10.00, 5, 2)                 |

  # Checked precision: a result that needs more digits is an error, never a
  # silent loss; rounding modes and bounded families don't mix.
  Scenario Outline: Decimal overflow and bad mixes are loud errors
    When I calculate "<expr>"
    Then the calculation fails mentioning "<message>"

    Examples:
      | expr                                                 | message  |
      | Decimal(12345, 4, 0)                                 | exceeds  |
      | Decimal(999.99, 5, 2) + Decimal(0.01, 5, 2)          | exceeds  |
      | Decimal(1, 5, 2) + Decimal(1, 5, 2, Rounding.HalfUp) | rounding |
      | Decimal(5, 5, 2) + Int8(5)                       | combine  |
      | Decimal(1, 1001, 2)                                  | between 1 and 1000 |

  # docs/DECIMAL.md — outside typed arithmetic a Decimal reads as the number it
  # represents: equality is numeric, it compares and sums like any number, and a
  # padded value equals its bare form (the trailing zero is presentation only).
  Scenario Outline: A Decimal coerces to its number outside typed arithmetic
    When I calculate "<expr>"
    Then the result is "<result>"

    Examples:
      | expr                                          | result |
      | Decimal(10.50, 5, 2) == 10.5                  | 1      |
      | Decimal(2.50, 5, 2) < 3                       | 1      |
      | sum(Decimal(1.5, 5, 2), Decimal(2.5, 5, 2))   | 4      |

  # docs/ANZAN.md §2 — truthiness is typed, never coerced.
  Scenario: A string is not a condition
    When I calculate "if("a", 1, 2)"
    Then the calculation fails mentioning "number"

  # docs/ANZAN.md §8 — empty ranges yield the operation's identity.
  Scenario: Empty reductions yield identities
    When I calculate "∑_i=1^0(i) + ∏_i=1^0(i)"
    Then the result is "1"

  Scenario: A reduction spanning too many terms is refused
    When I calculate "∑_i=1^200000(i)"
    Then the calculation fails mentioning "100,000"

  # docs/ANZAN.md §1 — reserved names refuse assignment (constants are
  # covered in mathematics.feature; these are the non-constant reserved words).
  Scenario Outline: Reserved words refuse assignment
    When I calculate "<expression>"
    Then the calculation fails mentioning "cannot assign"

    Examples:
      | expression |
      | ans = 5    |
      | sigma = 1  |
      | true = 0   |

  # docs/ANZAN.md §5 — parameters shadow globals, and the global survives.
  Scenario: Parameters shadow globals without clobbering them
    When I calculate "shade = 10"
    And I calculate "twice(shade) = shade * 2"
    And I calculate "twice(3) + shade"
    Then the result is "16"

  # docs/ANZAN.md §5 — a body's free variables read the CURRENT global at
  # each call, not a snapshot from definition time.
  Scenario: Free variables resolve at call time
    When I calculate "base = 10"
    And I calculate "above(y) = base + y"
    And I calculate "base = 100"
    And I calculate "above(1)"
    Then the result is "101"

  # docs/ANZAN.md §9 — special forms are documented like functions.
  Scenario: Special forms ship their manual
    When I calculate "man if"
    Then documentation is shown mentioning "taken branch"

  # docs/ANZAN.md §1 — escapes survive the round trip; strings index 0-based.
  Scenario: String escapes are honored
    When I calculate "len("a\tb")"
    Then the result is "3"

  # docs/ANZAN.md §1/§7 — `data` is contextual: only `data Name {` declares.
  Scenario: The word data is still an ordinary name
    When I calculate "data = 5"
    And I calculate "data * 2"
    Then the result is "10"

  # docs/ANZAN.md §7 — the single-letter compact lexing wrinkle: f(a:1) is
  # the cell reference a:1 (write f(a: 1)), while multi-letter compacts
  # decompose into named arguments.
  Scenario: Compact named arguments versus cell references
    Given I calculate "data Q { a: Number, age: Number }"
    When I calculate "Q(a: 1, age:36).age"
    Then the result is "36"
    # 0 proves a:1 stayed a (blank) CELL — a named argument would have made
    # sum() reject a map.
    When I calculate "sum(a:1)"
    Then the result is "0"
