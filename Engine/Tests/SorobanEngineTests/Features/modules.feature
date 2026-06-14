Feature: Namespaces group declarations under a qualified name
  As someone organizing types
  I want a namespace so generic names don't collide globally
  So that I can say Bits::Format instead of confiscating Format

  # docs/MODULES.md phase 2a-i: a `namespace Name { … }` block holds data
  # declarations, reached as Name::Type. Type identity is qualified, and a field
  # referencing a sibling type resolves within the namespace. (Namespaced
  # functions, imports, and persistence are later slices.)

  Scenario: A namespace exposes its data types by qualified name
    Given I calculate "namespace Geo { data Point { x: Number, y: Number } }"
    When I calculate "Geo::Point(x: 3, y: 4).x"
    Then the result is "3"

  Scenario: A namespaced record renders and re-parses with its qualified name
    Given I calculate "namespace Geo { data Point { x: Number, y: Number } }"
    When I calculate "Geo::Point(x: 3, y: 4)"
    Then the result is "Geo::Point(x: 3, y: 4)"

  Scenario: A field referencing a sibling type resolves within the namespace
    Given I calculate "namespace Geo { data Point { x: Number, y: Number } data Line { a: Point, b: Point } }"
    And I calculate "seg = Geo::Line(a: Geo::Point(x: 1, y: 1), b: Geo::Point(x: 4, y: 5))"
    When I calculate "seg.b.x"
    Then the result is "4"

  Scenario: List fields nest namespaced records
    Given I calculate "namespace Bits { data Field { name: String, flags: [String] } data Format { fields: [Field] } }"
    And I calculate "f = Bits::Format(fields: [Bits::Field(name: "owner", flags: ["r", "w", "x"])])"
    When I calculate "len(f.fields)"
    Then the result is "1"
    When I calculate "len(f.fields[0].flags)"
    Then the result is "3"

  Scenario Outline: Qualified construction validates and resolves loudly
    Given I calculate "namespace Bits { data Field { name: String } data Format { fields: [Field] } }"
    When I calculate "<expr>"
    Then the calculation fails mentioning "<message>"

    Examples:
      | expr                      | message          |
      | Bits::Format(fields: [5]) | is a Bits::Field |
      | Bits::Nope(x: 1)          | unknown function |

  Scenario: A namespace needs at least one declaration
    When I calculate "namespace Empty {  }"
    Then the calculation fails mentioning "at least one declaration"

  Scenario: A namespace currently holds only data declarations
    When I calculate "namespace Bad { x = 5 }"
    Then the calculation fails mentioning "only data declarations"
