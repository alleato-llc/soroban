Feature: The expanded function library
  As a spreadsheet refugee and a terminal dweller
  I want combinatorics, deeper statistics, amortization, business days, and bit math
  So that one exact engine covers what I'd otherwise scatter across tools

  # PROVENANCE: expected values in this file come from INDEPENDENT references
  # — Python's math/decimal modules (60-digit context) and hand derivation —
  # never from Soroban's own output. Full precision except where a result is
  # Double-bridged (solve, hyperbolics), which uses "within" bounds.

  Scenario Outline: Exact combinatorics — BigInt keeps every digit
    When I calculate "<expression>"
    Then the result is "<result>"

    Examples:
      | expression      | result                         |
      | choose(5, 2)    | 10                             |
      | choose(52, 5)   | 2598960                        |
      | choose(100, 50) | 100891344545564193334812497256 |
      | choose(2, 5)    | 0                              |
      | perm(5, 2)      | 20                             |
      | perm(10, 3)     | 720                            |
      | perm(10, 10)    | 3628800                        |

  Scenario Outline: Array plumbing
    When I calculate "<expression>"
    Then the result is "<result>"

    Examples:
      | expression                        | result               |
      | sort([3, 1, 2])                   | [1, 2, 3]            |
      | sort(["pear", "fig", "kiwi"])     | ["fig", "kiwi", "pear"] |
      | unique([3, 1, 3, 2, 1])           | [3, 1, 2]            |
      | reverse([1, 2, 3])                | [3, 2, 1]            |
      | reverse("abc")                    | "cba"                |
      | seq(1, 5)                         | [1, 2, 3, 4, 5]      |
      | seq(10, 0, -2)                    | [10, 8, 6, 4, 2, 0]  |
      | sum(map(x -> x^2, seq(1, 10)))    | 385                  |
      | list(1, 2, 3)                     | [1, 2, 3]            |
      | sumproduct([2, 3], [10, 100])     | 320                  |
      | sumproduct(1, 2, 3, 4, 5, 6)      | 32                   |

  Scenario: list() turns a range into an array — higher-order functions over cells
    Given the sheet contains:
      | cell | value |
      | A:1  | 5     |
      | A:2  | 12    |
      | A:3  | 8     |
      | A:4  | 30    |
    When I calculate "sum(filter(x -> x > 10, list(A:1..A:4)))"
    Then the result is "42"
    When I calculate "map(x -> x * 2, list(A:1..A:2))"
    Then the result is "[10, 24]"

  Scenario Outline: Statistics depth (sample conventions, full precision)
    When I calculate "<expression>"
    Then the result is "<result>"

    Examples:
      | expression                              | result                                              |
      | variance(2, 4, 4, 4, 5, 5, 7, 9)        | 4.5714285714285714285714285714285714285714285714286 |
      | mode(1, 2, 2, 3, 3, 3)                  | 3                                                   |
      | mode(4, 9, 4, 9)                        | 4                                                   |
      | percentile(15, 20, 35, 40, 50, 0.4)     | 29                                                  |
      | percentile(1, 2, 3, 4, 0.75)            | 3.25                                                |
      | percentile(1, 2, 3, 1)                  | 3                                                   |
      | geomean(4, 9)                           | 6                                                   |
      | correl(1, 2, 3, 2, 4, 6)                | 1                                                   |
      | slope(3, 5, 7, 1, 2, 3)                 | 2                                                   |
      | intercept(3, 5, 7, 1, 2, 3)             | 1                                                   |
      | forecast(4, 3, 5, 7, 1, 2, 3)           | 9                                                   |

  # PROVENANCE: Python decimal, 60-digit context — the amortization
  # identities below also pin ipmt + ppmt = pmt exactly, per period.
  Scenario Outline: Amortization and depreciation
    When I calculate "<expression>"
    Then the result is "<result>"

    Examples:
      | expression                                                                              | result   |
      | round(ipmt(0.05/12, 1, 360, 200000), 10)                                                | -833.3333333333 |
      | round(ipmt(0.05/12, 360, 360, 200000), 10)                                              | -4.4549512283 |
      | round(cumipmt(0.05/12, 360, 200000, 1, 12), 2)                                          | -9932.99 |
      | round(cumprinc(0.05/12, 360, 200000, 1, 12), 2)                                         | -2950.73 |
      | ipmt(0.05/12, 7, 360, 200000) + ppmt(0.05/12, 7, 360, 200000) == pmt(0.05/12, 360, 200000) | 1     |
      | sln(30000, 7500, 10)                                                                    | 2250     |
      | round(syd(30000, 7500, 10, 1), 2)                                                       | 4090.91  |
      | ddb(30000, 7500, 10, 1)                                                                 | 6000     |
      | ddb(30000, 7500, 10, 10)                                                                | 0        |
      | round(nominal(effectiveRate(0.06, 12), 12), 10)                                         | 0.06     |

  # 2026-06-05 is a Friday; June 2026 has 22 weekdays; 2026-01-01 is a
  # Thursday (hand-checked against a calendar).
  Scenario Outline: Business days and calendar positions
    When I calculate "<expression>"
    Then the result is "<result>"

    Examples:
      | expression                                                          | result |
      | quarter(date(2026, 6, 6))                                           | 2      |
      | quarter(date(2026, 11, 1))                                          | 4      |
      | weeknum(date(2026, 1, 1))                                           | 1      |
      | weeknum(date(2026, 1, 4))                                           | 2      |
      | workday(date(2026, 6, 5), 1) == date(2026, 6, 8)                    | 1      |
      | workday(date(2026, 6, 8), -1) == date(2026, 6, 5)                   | 1      |
      | networkdays(date(2026, 6, 1), date(2026, 6, 30))                    | 22     |
      | networkdays(date(2026, 6, 1), date(2026, 6, 30), date(2026, 6, 19)) | 21     |
      | networkdays(date(2026, 6, 5), date(2026, 6, 1))                     | -5     |

  Scenario Outline: Scientific completions
    When I calculate "<expression>"
    Then the result is "<result>"

    Examples:
      | expression       | result |
      | deg(pi)          | 180    |
      | deg(pi / 4)      | 45     |
      | sin(rad(90))     | 1      |
      | sinh(0)          | 0      |
      | cosh(0)          | 1      |
      | tanh(0)          | 0      |
      | asinh(0)         | 0      |
      | acosh(1)         | 0      |
      | atanh(0)         | 0      |

  Scenario: atan2 knows its quadrant
    When I calculate "atan2(1, 1) - pi/4"
    Then the result is within "1e-15" of zero
    When I calculate "atan2(-1, -1) + 3 * pi/4"
    Then the result is within "1e-15" of zero

  Scenario Outline: Programmer tools — exact at any width
    When I calculate "<expression>"
    Then the result is "<result>"

    Examples:
      | expression           | result                            |
      | toBase(255, 16)      | "FF"                              |
      | toBase(10, 2)        | "1010"                            |
      | fromBase("ff", 16)   | 255                               |
      | fromBase("1010", 2)  | 10                                |
      | toBase(-255, 16)     | "-FF"                             |
      | fromBase("-ff", 16)  | -255                              |
      | fromBase(toBase(123456789012345678901234567890, 36), 36) | 123456789012345678901234567890 |
      | bitAnd(12, 10)       | 8                                 |
      | bitOr(12, 10)        | 14                                |
      | bitXor(12, 10)       | 6                                 |
      | bitShift(1, 100)     | 1.267650600228229401496703205376e+30 |
      | bitShift(256, -4)    | 16                                |
      | bitAnd(0xFF, 0x0F)   | 15                                |
      | toBase(0b1111, 16)   | "F"                               |

  Scenario: solve() is goal seek in a formula
    When I calculate "solve(x -> x^2, 2) - sqrt(2)"
    Then the result is within "1e-12" of zero
    When I calculate "solve(cos, 0, 1) - pi/2"
    Then the result is within "1e-12" of zero
    # Saving $100/month to reach $20,000 in 10 years needs ~9.58% APR
    # (reference: independent bisection in Python).
    When I calculate "solve(x -> if(x < 1, -1, 1), 0) - 1"
    Then the result is within "1e-12" of zero
    When I calculate "solve(r -> fv(r, 120, -100), 20000, 0.01) * 12"
    Then the result is within "1e-9" of "0.0958092381724"

  # Self-hosting as PROOF, not runtime: the derived builtins must equal
  # their own definitions, exactly (== is exact). This is the audit in
  # docs/ANZAN.md "Design rules" made executable — bespoke Swift for
  # speed and error quality, pinned to the composition it stands for.
  # (True self-hosting was considered and refused: user functions can't
  # be variadic, and an AST walk per avg() call would tax every recalc.)
  Scenario Outline: Derived builtins equal their definitions
    When I calculate "<identity>"
    Then the result is "1"

    Examples:
      | identity                                                        |
      | avg(2, 4, 9) == sum(2, 4, 9) / count(2, 4, 9)                   |
      | stdev(2, 4, 4, 7) == sqrt(variance(2, 4, 4, 7))                 |
      | variance(2, 4) == ((2 - 3)^2 + (4 - 3)^2) / 1                   |
      | cbrt(27) == root(27, 3)                                         |
      | geomean(2, 4, 8) == root(product(2, 4, 8), 3)                   |
      | percent(8.25) == 8.25 / 100                                     |
      | deg(2) == 2 * 180 / pi                                          |
      | choose(10, 4) == fact(10) / (fact(4) * fact(6))                 |
      | perm(10, 4) == fact(10) / fact(6)                               |
      | lcm(12, 18) == 12 * 18 / gcd(12, 18)                            |
      | median(4, 1, 3) == sort([4, 1, 3])[1]                           |
      | sumproduct([2, 3], [10, 100]) == 2 * 10 + 3 * 100               |
      | npv(0.1, 300, 420) == 300 / 1.1 + 420 / 1.1^2                   |
      | forecast(4, 3, 5, 7, 1, 2, 3) == intercept(3, 5, 7, 1, 2, 3) + slope(3, 5, 7, 1, 2, 3) * 4 |
      | quarter(date(2026, 11, 1)) == trunc((month(date(2026, 11, 1)) - 1) / 3) + 1 |
      | bitShift(5, 3) == 5 * 2^3                                       |

  Scenario Outline: The new functions explain their mistakes
    When I calculate "<expression>"
    Then the calculation fails mentioning "<message>"

    Examples:
      | expression                      | message                    |
      | choose(-1, 2)                   | non-negative               |
      | sort([1, "a"])                  | all numbers or all strings |
      | seq(1, 10, 0)                   | can't be 0                 |
      | mode(1, 2, 3)                   | no value repeats           |
      | percentile(1, 2, 3, 1.5)        | between 0 and 1            |
      | geomean(4, -9)                  | positive                   |
      | correl(1, 2, 3, 4, 5)           | equal-length               |
      | ipmt(0.05, 0, 12, 1000)         | 1 ≤ per ≤ nper             |
      | syd(30000, 7500, 10, 11)        | 1 ≤ per ≤ life             |
      | toBase(1.5, 16)                 | integer                    |
      | fromBase("xyz", 16)             | not a base-16              |
      | bitAnd(-1, 2)                   | non-negative               |
      | solve(x -> x^2 + 1, 0)          | did not converge           |
      | ipmt(0.05, 1, 0, 1000)          | 1 ≤ per ≤ nper             |
      | cumipmt(0.05/12, 12, 1000, 5, 2) | 1 ≤ start ≤ end           |
      | nominal(-2, 12)                 | above -100%                |
      | ddb(30000, 7500, 10, 11)        | 1 ≤ per ≤ life             |
      | sln(1, 1, 0)                    | can't be 0                 |
      | toBase(255, 50)                 | 2–36                       |
      | fromBase("", 16)                | at least one digit         |
      | bitShift(1, 1.5)                | integer                    |
      | seq(1, 1000000)                 | 100,000                    |
      | workday(date(2026, 1, 1), 200001) | too many                 |
      | networkdays(date(1, 1, 1), date(9999, 1, 1)) | too many      |
