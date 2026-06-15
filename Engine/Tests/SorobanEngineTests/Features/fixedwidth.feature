Feature: Fixed-width integers are bounded and checked
  As a reader of docs/FIXED-WIDTH.md
  I want Int/UInt arithmetic to be exact and range-checked
  So that overflow is always a loud error, never a silent wraparound

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
