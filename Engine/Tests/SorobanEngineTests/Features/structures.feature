Feature: Strings, arrays, maps, and the functions that work them
  As a user with more than numbers
  I want text and structured data to be first-class
  So that records and lists live next to my math

  Scenario Outline: Text and structure functions
    When I calculate "<expression>"
    Then the result is "<result>"

    Examples:
      | expression                            | result          |
      | len([1, 2, 3])                        | 3               |
      | len("hello")                          | 5               |
      | len({a: 1, b: 2})                     | 2               |
      | first([5, 6, 7])                      | 5               |
      | last([5, 6, 7])                       | 7               |
      | keys({name: "Ada", age: 36})          | ["name", "age"] |
      | values({a: 1, b: 2})                  | [1, 2]          |
      | sum(values({a: 1, b: 2}))             | 3               |
      | concat("Q", 1, "-", 2026)             | "Q1-2026"       |
      | concat([1, 2], [3])                   | [1, 2, 3]       |
      | "Q" + 1                               | "Q1"            |
      | "a" + "b" + "c"                       | "abc"           |
      | "abc"[0]                              | "a"             |
      | [[1, 2], [3, 4]][1][0]                | 3               |
      | {a: [1, 2]}.a[1]                      | 2               |
      | [1, 2] == [1, 2]                      | 1               |
      | {a: 1, b: 2} == {b: 2, a: 1}          | 1               |
      | "x" != 5                              | 1               |

  Scenario Outline: Structure mistakes explain themselves
    When I calculate "<expression>"
    Then the calculation fails mentioning "<message>"

    Examples:
      | expression       | message              |
      | [1, 2][5]        | out of range         |
      | {a: 1}.b         | no key 'b'           |
      | first([])        | empty array          |
      | sum(["a"])       | works on numbers     |
      | "a" * 2          | expected a number    |
      | {a: 1, a: 2}     | duplicate key        |

  Scenario: Filter and reduce shape data like a user thinks
    When I calculate "filter(x -> x > 1, [1, 2, 3])"
    Then the result is "[2, 3]"
    When I calculate "reduce((a, b) -> a + b, [], 42)"
    Then the result is "42"

  Scenario: Lambdas live in variables and close over parameters
    When I calculate "f = x -> x * 2"
    And I calculate "f(21)"
    Then the result is "42"
    When I calculate "scale(arr, n) = map(x -> x * n, arr)"
    And I calculate "scale([1, 2, 3], 10)"
    Then the result is "[10, 20, 30]"

  Scenario: A named function reference follows redefinition
    When I calculate "h(x) = x + 1"
    And I calculate "alias = h"
    And I calculate "h(x) = x + 100"
    And I calculate "alias(1)"
    Then the result is "101"
