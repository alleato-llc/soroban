Feature: Namespaces group declarations under a qualified name
  As someone organizing types and functions
  I want a namespace so generic names don't collide globally
  So that I can say Bits::Format instead of confiscating Format

  # docs/MODULES.md phase 2a: a `namespace Name { … }` block holds data and
  # function declarations, separated by ';', reached as Name::member. Type
  # identity is qualified; a member references its siblings unqualified (resolved
  # at call time via the home-namespace context), including parameter types.
  # (Constants, imports, nesting, and persistence are later slices.)

  Scenario: A namespace exposes its data types by qualified name
    Given I calculate "namespace Geo { data Point { x: Number, y: Number } }"
    When I calculate "Geo::Point(x: 3, y: 4).x"
    Then the result is "3"

  Scenario: A namespaced record renders and re-parses with its qualified name
    Given I calculate "namespace Geo { data Point { x: Number, y: Number } }"
    When I calculate "Geo::Point(x: 3, y: 4)"
    Then the result is "Geo::Point(x: 3, y: 4)"

  Scenario: A field referencing a sibling type resolves within the namespace
    Given I calculate "namespace Geo { data Point { x: Number, y: Number }; data Line { a: Point, b: Point } }"
    And I calculate "seg = Geo::Line(a: Geo::Point(x: 1, y: 1), b: Geo::Point(x: 4, y: 5))"
    When I calculate "seg.b.x"
    Then the result is "4"

  Scenario: List fields nest namespaced records
    Given I calculate "namespace Bits { data Field { name: String, flags: [String] }; data Format { fields: [Field] } }"
    And I calculate "f = Bits::Format(fields: [Bits::Field(name: "owner", flags: ["r", "w", "x"])])"
    When I calculate "len(f.fields)"
    Then the result is "1"
    When I calculate "len(f.fields[0].flags)"
    Then the result is "3"

  Scenario: A namespaced function resolves sibling functions and types unqualified
    Given I calculate "namespace Geo { data Point { x: Number, y: Number }; dist(p: Point) = sqrt(p.x^2 + p.y^2); origin() = Point(x: 0, y: 0) }"
    When I calculate "Geo::dist(Geo::Point(x: 3, y: 4))"
    Then the result is "5"
    When I calculate "Geo::dist(Geo::origin())"
    Then the result is "0"

  Scenario: A namespaced function calls a sibling function unqualified
    Given I calculate "namespace M { a(n) = b(n) + 1; b(n) = n * 2 }"
    When I calculate "M::a(10)"
    Then the result is "21"

  Scenario: Namespaced recursion resolves itself through the home namespace
    Given I calculate "namespace R { fact(n) = if(n <= 1, 1, n * fact(n - 1)) }"
    When I calculate "R::fact(6)"
    Then the result is "720"

  Scenario Outline: Qualified construction validates and resolves loudly
    Given I calculate "namespace Bits { data Field { name: String }; data Format { fields: [Field] } }"
    When I calculate "<expr>"
    Then the calculation fails mentioning "<message>"

    Examples:
      | expr                      | message          |
      | Bits::Format(fields: [5]) | is a Bits::Field |
      | Bits::Nope(x: 1)          | unknown function |

  Scenario: A namespace needs at least one declaration
    When I calculate "namespace Empty {  }"
    Then the calculation fails mentioning "at least one declaration"

  Scenario: Namespace members are separated by a semicolon
    When I calculate "namespace Bad { data A { x: Number } data B { y: Number } }"
    Then the calculation fails mentioning "separate namespace declarations with ';'"

  Scenario: Constants and nesting are not in a namespace yet
    When I calculate "namespace Bad { x = 5 }"
    Then the calculation fails mentioning "constants and nesting come later"

  # docs/MODULES.md 2b: `import Name` brings a namespace's members into scope
  # unqualified; the qualified form always works; conflicts are loud.

  Scenario: import brings a namespace's members into scope unqualified
    Given I calculate "namespace Geo { data Point { x: Number, y: Number }; dist(p: Point) = sqrt(p.x^2 + p.y^2) }"
    And I calculate "import Geo"
    When I calculate "dist(Point(x: 3, y: 4))"
    Then the result is "5"
    When I calculate "Geo::dist(Geo::Point(x: 6, y: 8))"
    Then the result is "10"

  Scenario: Importing an unknown namespace errors
    When I calculate "import Nope"
    Then the calculation fails mentioning "no namespace 'Nope'"

  Scenario: Importing a name that collides with a global is loud
    Given I calculate "data Pt { x: Number }"
    And I calculate "namespace NS { data Pt { x: Number } }"
    When I calculate "import NS"
    Then the calculation fails mentioning "would shadow 'Pt'"

  Scenario: Importing a name that collides with a builtin is loud
    Given I calculate "namespace M2 { sqrt(x) = x * x }"
    When I calculate "import M2"
    Then the calculation fails mentioning "would shadow 'sqrt'"

  Scenario: Re-importing a namespace is a harmless no-op
    Given I calculate "namespace Ok { area(r) = 3 * r * r }"
    And I calculate "import Ok"
    And I calculate "import Ok"
    When I calculate "area(2)"
    Then the result is "12"

  # docs/MODULES.md 2c: namespaces (their declaration lines) and imports persist
  # in the workbook and replay on open, in dependency order.

  Scenario: A namespace and its import survive save and reopen
    Given I calculate "namespace Geo { data Point { x: Number, y: Number }; dist(p: Point) = sqrt(p.x^2 + p.y^2) }"
    And I calculate "import Geo"
    When the workbook is saved and reopened
    And I calculate "Geo::dist(Geo::Point(x: 3, y: 4))"
    Then the result is "5"
    When I calculate "dist(Point(x: 6, y: 8))"
    Then the result is "10"
