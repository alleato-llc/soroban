Feature: Fixed-precision decimals are SQL-style money values
  As a reader of docs/DECIMAL.md
  I want Decimal(p, s) to round to scale and check precision
  So that money math never silently loses digits

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
