Feature: The mathematics a user can reach
  As a user of one calculator for everything
  I want every domain — algebra, trig, stats, finance, dates — to answer correctly
  So that I never need a second tool

  Scenario Outline: Core arithmetic and algebra
    When I calculate "<expression>"
    Then the result is "<result>"

    Examples:
      | expression        | result |
      | 1 + 2 * 3         | 7      |
      | (1 + 2) * 3       | 9      |
      | 10 / 4            | 2.5    |
      | mod(7, 3)         | 1      |
      | -2^2              | -4     |
      | 2^-1              | 0.5    |
      | abs(-5)           | 5      |
      | min(3, 1, 2)      | 1      |
      | max(3, 1, 2)      | 3      |
      | floor(2.7)        | 2      |
      | floor(-1.5)       | -2     |
      | ceil(2.1)         | 3      |
      | ceil(-1.5)        | -1     |
      | trunc(-2.7)       | -2     |
      | round(2.345, 2)   | 2.34   |
      | round(2.567, 2)   | 2.57   |
      | round(2.5)        | 2      |
      | mod(7, 3)         | 1      |
      | fact(5)           | 120    |
      | fact(20)          | 2432902008176640000 |
      | gcd(12, 18)       | 6      |
      | lcm(4, 6)         | 12     |
      | percent(8.25)     | 0.0825 |
      | sqrt(144)         | 12     |
      | cbrt(27)          | 3      |
      | cbrt(-27)         | -3     |
      | root(32, 5)       | 2      |
      | pow(4, 0.5)       | 2      |

  Scenario Outline: Mathematical symbols are first-class
    When I calculate "<expression>"
    Then the result is "<result>"

    Examples:
      | expression  | result |
      | √16                | 4  |
      | √(2 + 2)           | 2  |
      | √2^2               | 2  |
      | 6 × 7 ÷ 2          | 21 |
      | 6 ÷ 2 − 1          | 2  |
      | 3 · 4              | 12 |
      | π − pi             | 0  |
      | τ ÷ π              | 2  |
      | pi                 | 3.14159265358979323846264338327950288419716939937510582097494 |
      | 3 ≠ 2              | 1  |
      | 2 ≤ 2              | 1  |

  Scenario: Constants are protected
    When I calculate "π = 3"
    Then the calculation fails mentioning "cannot assign to 'π'"

  Scenario Outline: Trigonometry (radians)
    When I calculate "<expression>"
    Then the result is "<result>"

    # Full precision, no rounding: these results expose the engine's
    # documented Double seam (transcendentals carry ~15-17 digits) exactly
    # as a user sees it — tan(π/4) really does show 0.9999999999999999.
    Examples:
      | expression   | result             |
      | sin(pi / 2)  | 1                  |
      | cos(pi)      | -1                 |
      | tan(pi / 4)  | 0.9999999999999999 |
      | asin(1) * 2  | 3.1415926535897932 |
      | acos(0) * 2  | 3.1415926535897932 |
      | atan(1) * 4  | 3.1415926535897932 |

  Scenario Outline: Exponentials and logarithms
    When I calculate "<expression>"
    Then the result is "<result>"

    Examples:
      | expression    | result            |
      | exp(0)        | 1                 |
      | ln(e)         | 1                 |
      | log10(1000)   | 3                 |
      | log(2, 8)     | 3                 |
      | sin(0)        | 0                 |
      | cos(0)        | 1                 |

  # Stated to the Double seam's precision, not bit-exactness: platform libm
  # implementations legitimately differ by an ulp here (Darwin's exp gives
  # 6.999999999999999, pure-Rust libm exactly 7) — the cross-implementation
  # tolerance rule from docs/MIGRATION.md §3.
  Scenario: exp inverts ln to within the Double seam
    When I calculate "exp(ln(7))"
    Then the result is within "0.000000000000002" of "7"

  Scenario Outline: Statistics over lists, arrays, and ranges
    When I calculate "<expression>"
    Then the result is "<result>"

    Examples:
      | expression                            | result |
      | avg(2, 4, 9)                          | 5      |
      | median(1, 9, 5)                       | 5      |
      | median(1, 2, 3, 4)                    | 2.5    |
      | count(1, 2, 3)                        | 3      |
      | product(2, 3, 4)                      | 24     |
      | stdev(2, 4, 4, 4, 5, 5, 7, 9)         | 2.1380899352993950774764278470380281724320113187307 |
      | avg([2, 4, 9])                        | 5      |
      | stdev(1, 3) - sqrt(2)                 | 0      |

  Scenario: Products mirror summation
    When I calculate "∏(2, 3, 4)"
    Then the result is "24"
    When I calculate "∏_i=1^5(i)"
    Then the result is "120"

  Scenario: Euler's number from its factorial series
    When I calculate "∑_k=0^45(1 / fact(k)) - e"
    Then the result is within "1e-45" of zero

  Scenario: Nicomachus — the sum of cubes is the square of the sum
    When I calculate "reduce((a, b) -> a + b, map(x -> x^3, [1,2,3,4,5,6,7,8,9,10]), 0) == (∑_i=1^10(i))^2"
    Then the result is "1"

  Scenario: Three roads to twenty factorial agree exactly
    When I calculate "rec(n) = if(n <= 1, 1, n * rec(n - 1))"
    And I calculate "∏_i=1^20(i) == fact(20)"
    Then the result is "1"
    When I calculate "rec(20) == fact(20)"
    Then the result is "1"

  Scenario: Compound growth's product form equals its closed form, exactly
    When I calculate "grow(r, n) = ∏_i=1^(n)(1 + r)"
    And I calculate "grow(0.05, 30) == 1.05^30"
    Then the result is "1"

  Scenario: A mortgage payment round-trips the principal to 30 decimal places
    When I calculate "pv(0.05/12, 360, pmt(0.05/12, 360, 200000)) - 200000"
    Then the result is within "1e-30" of zero

  Scenario Outline: Finance agrees with Excel (independent cross-validation)
    # PROVENANCE: every expected value below was read from EXCEL, not from
    # the engine — that is this scenario's entire value. The tolerance is
    # the precision the reference was read at. NEVER refresh these numbers
    # from Soroban's own output: if this fails and Excel agrees with the
    # table, Soroban is wrong.
    When I calculate "<expression>"
    Then the result is within "<tolerance>" of "<expected>"

    Examples:
      | expression                                      | expected   | tolerance |
      | pmt(0.05/12, 360, 200000)                       | -1073.64   | 0.01      |
      | pmt(0, 12, 1200)                                | -100       | 0.000001  |
      | fv(0.06/12, 120, -100)                          | 16387.93   | 0.01      |
      | fv(0, 12, -100)                                 | 1200       | 0.000001  |
      | pv(0.04/12, 60, -500)                           | 27149.53   | 0.01      |
      | nper(0.05/12, -1073.64, 200000)                 | 360        | 0.01      |
      | nper(0, -100, 1200)                             | 12         | 0.000001  |
      | rate(360, -1073.64, 200000) * 12                | 0.05       | 0.0001    |
      | npv(0.1, 3000, 4200, 6800)                      | 11307.29   | 0.01      |
      | irr(-70000, 12000, 15000, 18000, 21000, 26000)  | 0.0866     | 0.0001    |
      | effectiveRate(0.06, 12)                         | 0.061678   | 0.000001  |
      | markup(80, 25)                                  | 100        | 0.000001  |
      | percentOf(30, 120)                              | 25         | 0.000001  |
      | percentChange(80, 100)                          | 25         | 0.000001  |

  Scenario: A mortgage's lifetime cost, built up across lines
    When I calculate "r = 0.05 / 12"
    And I calculate "payment = pmt(r, 360, 200000)"
    And I calculate "payment * 360"
    Then the result is within "0.01" of "-386511.57"

  Scenario: IRR refuses an unsolvable stream
    When I calculate "irr(1000, 2000)"
    Then the calculation fails mentioning "both positive and negative"

  Scenario Outline: Finance (spreadsheet sign convention)
    When I calculate "<expression>"
    Then the result is "<result>"

    Examples:
      | expression                                        | result  |
      | fv(0.1, 2, -100)                                  | 210     |
      | npv(0, 10, 20, 30)                                | 60      |
      | npv(0.1, 110)                                     | 100     |
      | nper(0, -10, 100)                                 | 10      |
      | irr(-100, 110)                                    | 0.1     |
      | effectiveRate(0.12, 12)                           | 0.126825030131969720661201 |

  Scenario Outline: Accounting shorthands
    When I calculate "<expression>"
    Then the result is "<result>"

    Examples:
      | expression               | result |
      | margin(100, 80)          | 20     |
      | markup(80, 25)           | 100    |
      | percentOf(50, 200)       | 25     |
      | percentChange(100, 150)  | 50     |

  Scenario Outline: Date arithmetic on exact day serials
    When I calculate "<expression>"
    Then the result is "<result>"

    Examples:
      | expression                                   | result |
      | weekday(date(2026, 6, 6))                    | 6      |
      | weekday(date(2026, 6, 8))                    | 1      |
      | year(date(2026, 6, 6))                       | 2026   |
      | month(date(2026, 6, 6))                      | 6      |
      | day(date(2026, 6, 6))                        | 6      |
      | days(date(2026, 3, 1), date(2026, 2, 1))     | 28     |
      | edate(date(2026, 1, 31), 1) == date(2026, 2, 28) | 1  |
      | eomonth(date(2024, 2, 1), 0) == date(2024, 2, 29) | 1 |
      | day(eomonth(date(1900, 2, 1), 0))            | 28     |
      | day(eomonth(date(2000, 2, 1), 0))            | 29     |
      | days(date(2001, 1, 1), date(2000, 1, 1))     | 366    |

  Scenario: Logic composes with everything
    When I calculate "and(1 < 2, or(0, not(0)))"
    Then the result is "1"
