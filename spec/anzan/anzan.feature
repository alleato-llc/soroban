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

  # docs/ANZAN.md §12 — $ pins are copy-time data for fill and paste; to the
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

  # docs/ANZAN.md §5 — modulo is the mod() function (the % operator is percent),
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

  # docs/ANZAN.md §2 — truthiness is typed, never coerced.
  Scenario: A string is not a condition
    When I calculate "if("a", 1, 2)"
    Then the calculation fails mentioning "number"

  # docs/ANZAN.md §10 — empty ranges yield the operation's identity.
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

  # docs/ANZAN.md §6 — parameters shadow globals, and the global survives.
  Scenario: Parameters shadow globals without clobbering them
    When I calculate "shade = 10"
    And I calculate "twice(shade) = shade * 2"
    And I calculate "twice(3) + shade"
    Then the result is "16"

  # docs/ANZAN.md §6 — a body's free variables read the CURRENT global at
  # each call, not a snapshot from definition time.
  Scenario: Free variables resolve at call time
    When I calculate "base = 10"
    And I calculate "above(y) = base + y"
    And I calculate "base = 100"
    And I calculate "above(1)"
    Then the result is "101"

  # docs/ANZAN.md §11 — special forms are documented like functions.
  Scenario: Special forms ship their manual
    When I calculate "man if"
    Then documentation is shown mentioning "taken branch"

  # docs/ANZAN.md §1 — escapes survive the round trip; strings index 0-based.
  Scenario: String escapes are honored
    When I calculate "len("a\tb")"
    Then the result is "3"

  # docs/ANZAN.md §1/§8 — `data` is contextual: only `data Name {` declares.
  Scenario: The word data is still an ordinary name
    When I calculate "data = 5"
    And I calculate "data * 2"
    Then the result is "10"

  # docs/ANZAN.md §8 — the single-letter compact lexing wrinkle: f(a:1) is
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
