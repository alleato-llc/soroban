Feature: Defining your own functions
  As a user with my own formulas
  I want to define, compose, and document functions
  So that the calculator speaks my domain

  Scenario: Define a function and use it
    When I calculate "tax(x) = x * 1.0825  # TX sales tax"
    And I calculate "tax(100)"
    Then the result is "108.25"

  Scenario: Functions compose regardless of definition order
    When I calculate "g(x) = f(x) + 1"
    And I calculate "f(x) = x * 2"
    And I calculate "g(20)"
    Then the result is "41"

  Scenario: Recursion works
    When I calculate "fact(n) = if(n <= 1, 1, n * fact(n - 1))"
    And I calculate "fact(10)"
    Then the result is "3628800"

  Scenario: Lambdas are values
    When I calculate "map(x -> x * 2, [1, 2, 3])"
    Then the result is "[2, 4, 6]"

  Scenario: Structures carry data
    When I calculate "people = [{name: "Ada", age: 36}, {name: "Bob", age: 32}]"
    And I calculate "people[0].age"
    Then the result is "36"

  Scenario: Machin's 1706 formula recovers pi
    When I calculate "arctanInv(x, n) = ∑_k=0^(n)((-1)^k / ((2k + 1) * x^(2k + 1)))"
    And I calculate "16 * arctanInv(5, 40) - 4 * arctanInv(239, 15) - pi"
    Then the result is within "1e-45" of zero

  Scenario: Comments are for humans and ignored by the math
    When I calculate "6 * 7  # the answer"
    Then the result is "42"

  Scenario: A trailing comment becomes the function's documentation
    When I calculate "tax(x) = x * 1.0825  # TX sales tax"
    And I calculate "man(tax)"
    Then documentation is shown mentioning "TX sales tax"

  Scenario: Built-in functions ship their manual
    When I calculate "man(pmt)"
    Then documentation is shown mentioning "payment"

  Scenario: Only the taken branch of if() runs
    When I calculate "if(1, 2, 1/0)"
    Then the result is "2"

  Scenario: Deep recursion is bounded by memory, not a counter
    # The old fixed depth limit (40) refused honest recursion like fib(50);
    # evaluation now hops to fresh stack segments as needed.
    When I calculate "countdown(n) = if(n <= 0, 0, countdown(n - 1) + 1)"
    And I calculate "countdown(2000)"
    Then the result is "2000"

  Scenario: Tail-recursive functions run at constant stack — any depth
    # The recursive call is the WHOLE result of the taken branch, so the
    # evaluator loops instead of stacking (real tail-call optimization).
    # Half a million self-calls, no growing stack, exact answer.
    When I calculate "sumTo(n, acc) = if(n <= 0, acc, sumTo(n - 1, acc + n))"
    And I calculate "sumTo(500000, 0)"
    Then the result is "125000250000"

  Scenario: Forgetting a base case fails politely, with a hint
    # The user's exact transcript: F(n) = F(n-1) + F(n-2) never terminates.
    When I calculate "F(n) = F(n - 1) + F(n - 2)"
    And I calculate "f(3)"
    Then the calculation fails mentioning "base case"

  Scenario: Runaway recursion is cut off cleanly, not a crash
    When I calculate "loop(n) = loop(n + 1)"
    And I calculate "loop(1)"
    Then the calculation fails mentioning "nested too deeply"

  Scenario: Built-in names are protected
    When I calculate "abs(x) = x"
    Then the calculation fails mentioning "built-in"

  Scenario: Function calls are case-insensitive
    When I calculate "MIN(1, 2) + Sqrt(16)"
    Then the result is "5"
