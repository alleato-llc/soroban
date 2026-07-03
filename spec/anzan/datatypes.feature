Feature: Data types — declared records with named construction
  As a user modeling real things
  I want to declare a typed record once and build instances by field name
  So that my collections stay consistent and serialize honestly

  # Reminder: Gherkin has no escape for inner quotes — they ride the greedy
  # "(.*)" capture bare. A \" here would put a literal backslash in the text.

  Scenario: Declaring a type registers its constructor
    When I calculate "data Person { name: String, age: Number, active: Boolean }"
    Then the result is "data Person { name: String, age: Number, active: Boolean }"
    When I calculate "Person(name: "Ada", age: 36, active: true)"
    Then the result is "Person(name: "Ada", age: 36, active: true)"

  Scenario: Construction is named fields or one map — never positional
    Given I calculate "data Pt { x: Number, y: Number }"
    When I calculate "Pt(y: 4, x: 3)"
    Then the result is "Pt(x: 3, y: 4)"
    When I calculate "m = {x: 3, y: 4}"
    And I calculate "Pt(m)"
    Then the result is "Pt(x: 3, y: 4)"
    When I calculate "Pt(3, 4)"
    Then the calculation fails mentioning "takes named fields"

  Scenario: Field types accept any casing and Boolean fields render true/false
    When I calculate "data Flag { on: boolean }"
    Then the result is "data Flag { on: Boolean }"
    When I calculate "Flag(on: 1 < 2)"
    Then the result is "Flag(on: true)"

  Scenario: Fields read like map members, by dot or by key
    Given I calculate "data Pt { x: Number, y: Number }"
    And I calculate "p = Pt(x: 3, y: 4)"
    When I calculate "sqrt(p.x^2 + p.y^2)"
    Then the result is "5"
    When I calculate "p["y"]"
    Then the result is "4"
    When I calculate "keys(p)"
    Then the result is "["x", "y"]"
    When I calculate "len(p)"
    Then the result is "2"

  Scenario: Records collect into arrays and flow through higher-order functions
    Given I calculate "data Person { name: String, age: Number, active: Boolean }"
    And I calculate "team = [Person(name: "Ada", age: 36, active: true), Person(name: "Grace", age: 30, active: false)]"
    When I calculate "map(x -> x.age, team)"
    Then the result is "[36, 30]"
    When I calculate "sum(map(x -> x.age, team))"
    Then the result is "66"
    When I calculate "filter(x -> x.active, team)"
    Then the result is "[Person(name: "Ada", age: 36, active: true)]"
    When I calculate "map(Person, [{name: "Bo", age: 1, active: true}])"
    Then the result is "[Person(name: "Bo", age: 1, active: true)]"

  Scenario: Instances canonicalize to declaration order and compare deeply
    Given I calculate "data Pt { x: Number, y: Number }"
    When I calculate "Pt(y: 2, x: 1) == Pt(x: 1, y: 2)"
    Then the result is "1"
    When I calculate "Pt(x: 1, y: 2) == {x: 1, y: 2}"
    Then the result is "0"

  Scenario: toJson is pretty by default, compact on request, honest about Booleans
    Given I calculate "data Person { name: String, age: Number, active: Boolean }"
    And I calculate "p = Person(name: "Ada", age: 36, active: true)"
    When I calculate "toJson(p)"
    Then the result is ""{\n  \"name\": \"Ada\",\n  \"age\": 36,\n  \"active\": true\n}""
    When I calculate "toJson(p, Json.Compact)"
    Then the result is ""{\"name\":\"Ada\",\"age\":36,\"active\":true}""
    When I calculate "toJson([1, 2])"
    Then the result is ""[\n  1,\n  2\n]""

  Scenario: fromJson parses JSON into values, exactly
    When I calculate "fromJson("[1, 2, 3]")"
    Then the result is "[1, 2, 3]"
    When I calculate "fromJson("{\"name\": \"Ada\", \"age\": 36}").age"
    Then the result is "36"
    # Numbers parse at full precision — never through floating point.
    When I calculate "fromJson("0.30000000000000004") == 0.30000000000000004"
    Then the result is "1"
    When I calculate "fromJson("123456789012345678901234567890.5") * 2"
    Then the result is "246913578024691357802469135781"
    When I calculate "fromJson("true") + fromJson("false")"
    Then the result is "1"
    When I calculate "fromJson("\"\\u0041da\"")"
    Then the result is ""Ada""

  Scenario: fromJson and a constructor re-type a serialized record
    Given I calculate "data Person { name: String, age: Number, active: Boolean }"
    And I calculate "p = Person(name: "Ada", age: 36, active: true)"
    When I calculate "Person(fromJson(toJson(p))) == p"
    Then the result is "1"
    When I calculate "map(Person, fromJson(toJson([p, p])))[1].age"
    Then the result is "36"

  Scenario Outline: fromJson mistakes explain themselves
    When I calculate "<expression>"
    Then the calculation fails mentioning "<message>"

    Examples:
      | expression                     | message                  |
      | fromJson("null")               | has no Anzan value       |
      | fromJson("[1,")                | unexpected end           |
      | fromJson("[1] junk")           | trailing content         |
      | fromJson("{\"a\":1,\"a\":2}")  | duplicate key            |
      | fromJson(5)                    | wants JSON text          |

  Scenario: Json options are named constants, not magic flags
    When I calculate "Json.Pretty"
    Then the result is ""pretty""
    When I calculate "toJson([1, 2], Json.Compact)"
    Then the result is ""[1,2]""
    When I calculate "toJson([1, 2], "compact")"
    Then the result is ""[1,2]""
    When I calculate "Json = 5"
    Then the calculation fails mentioning "cannot assign"
    When I calculate "man Json"
    Then documentation is shown mentioning "Formatting options"

  Scenario Outline: Construction mistakes explain themselves
    Given I calculate "data Person { name: String, age: Number, active: Boolean }"
    When I calculate "<expression>"
    Then the calculation fails mentioning "<message>"

    Examples:
      | expression                                       | message                      |
      | Person(name: "Ada", age: 36)                     | missing 'active'             |
      | Person(name: 7, age: 36, active: true)           | 'name' of Person is a String |
      | Person(name: "A", age: "x", active: true)        | 'age' of Person is a Number  |
      | Person(name: "A", age: 36, active: 7)            | use true or false            |
      | Person(name: "A", age: 36, active: true, pet: 1) | no field 'pet'               |
      | Person(name: "A", name: "B", age: 1, active: 1)  | duplicate field              |
      | toJson(sqrt)                                     | can't serialize a function   |
      | toJson(1, "wat")                                 | unknown toJson option        |
      | toJson(1, true)                                  | Json.Pretty or Json.Compact  |

  Scenario Outline: Declaration mistakes explain themselves
    When I calculate "<expression>"
    Then the calculation fails mentioning "<message>"

    Examples:
      | expression                        | message                    |
      | data person { a: Number }         | capital letter             |
      | data Bad { a: truthy }            | declared data type         |
      | data Empty {}                     | at least one field         |
      | data Dup { a: Number, A: Number } | duplicate field            |
      | data Abs { a: Number }            | built-in                   |

  Scenario: Types and functions can't share a name, but redeclaring your own type works
    Given I calculate "f(x) = x + 1"
    When I calculate "data F { a: Number }"
    Then the calculation fails mentioning "already a function"
    When I calculate "data G { a: Number }"
    And I calculate "g(x) = x"
    Then the calculation fails mentioning "is a data type"
    When I calculate "data G { a: Number, b: Number }"
    And I calculate "G(a: 1, b: 2).b"
    Then the result is "2"

  Scenario: A type documents itself through its trailing comment
    When I calculate "data Invoice { total: Number, paid: Boolean } # one customer invoice"
    And I calculate "man Invoice"
    Then documentation is shown mentioning "one customer invoice"

  Scenario: toJson escapes strings and renders empty containers
    When I calculate "toJson("a\tb")"
    Then the result is ""\"a\\tb\"""
    When I calculate "toJson([])"
    Then the result is ""[]""
    When I calculate "toJson({})"
    Then the result is ""{}""

  Scenario: A constructor call works in tail position of a recursive function
    Given I calculate "data Box { v: Number }"
    And I calculate "wrap(n) = if(n <= 0, Box(v: n), wrap(n - 1))"
    When I calculate "wrap(5).v"
    Then the result is "0"

  Scenario: The explicit formula marker rejects declarations in cells
    Given cell A:1 contains "=data Pt { x: Number }"
    Then cell A:1 shows an error mentioning "drop the leading '='"

  Scenario: A plain declaration in a cell is a sheet-scoped 𝑫 type
    Given cell A:1 contains "data Pt { x: Number, y: Number }"
    And cell B:1 contains "Pt(x: 3, y: 4).x"
    Then cell A:1 shows "𝑫 Pt"
    And cell B:1 shows "3"
    When I calculate "data Pt { x: Number }"
    Then the calculation fails mentioning "defined in cell Sheet 1!A:1"

  Scenario: Types and record variables survive save and reopen
    Given I calculate "data Person { name: String, age: Number, active: Boolean }"
    And I calculate "p = Person(name: "Ada", age: 36, active: true)"
    When the workbook is saved and reopened
    And I calculate "p.age + Person(name: "B", age: 4, active: false).age"
    Then the result is "40"

  Scenario: A typed parameter dispatches a function by argument type
    Given I calculate "kind(n: Number) = "number""
    And I calculate "kind(s: String) = "string""
    When I calculate "kind(42)"
    Then the result is ""number""
    When I calculate "kind("hi")"
    Then the result is ""string""

  Scenario: A function can be written for a data type
    Given I calculate "data Point { x: Number, y: Number }"
    And I calculate "midpoint(a: Point, b: Point) = Point(x: (a.x + b.x) / 2, y: (a.y + b.y) / 2)"
    When I calculate "midpoint(Point(x: 0, y: 0), Point(x: 4, y: 10))"
    Then the result is "Point(x: 2, y: 5)"

  Scenario: An operator can be overloaded for a data type
    Given I calculate "data Point { x: Number, y: Number }"
    And I calculate "+(a: Point, b: Point) = Point(x: a.x + b.x, y: a.y + b.y)"
    When I calculate "Point(x: 1, y: 2) + Point(x: 10, y: 20)"
    Then the result is "Point(x: 11, y: 22)"

  Scenario: An overloaded operator can mix a data type and a scalar
    Given I calculate "data Point { x: Number, y: Number }"
    And I calculate "*(a: Point, s: Number) = Point(x: a.x * s, y: a.y * s)"
    When I calculate "Point(x: 1, y: 2) * 3"
    Then the result is "Point(x: 3, y: 6)"

  Scenario: Built-in arithmetic is untouched by an overload
    Given I calculate "data Point { x: Number, y: Number }"
    And I calculate "+(a: Point, b: Point) = Point(x: a.x + b.x, y: a.y + b.y)"
    When I calculate "1 + 2"
    Then the result is "3"
    When I calculate ""Q" + 1"
    Then the result is ""Q1""

  Scenario: An operator overload must involve a data type
    When I calculate "+(a: Number, b: Number) = 5"
    Then the calculation fails mentioning "must involve a data type"

  Scenario: Records compare equal by all of their state
    Given I calculate "data Point { x: Number, y: Number }"
    When I calculate "Point(x: 1, y: 2) == Point(x: 1, y: 2)"
    Then the result is "1"
    When I calculate "Point(x: 1, y: 2) == Point(x: 9, y: 9)"
    Then the result is "0"
    When I calculate "Point(x: 1, y: 2) != Point(x: 9, y: 9)"
    Then the result is "1"

  Scenario: Operator overloads survive save and reopen
    Given I calculate "data Point { x: Number, y: Number }"
    And I calculate "+(a: Point, b: Point) = Point(x: a.x + b.x, y: a.y + b.y)"
    And I calculate "*(a: Point, s: Number) = Point(x: a.x * s, y: a.y * s)"
    When the workbook is saved and reopened
    And I calculate "(Point(x: 1, y: 1) + Point(x: 2, y: 3)).y"
    Then the result is "4"
    And I calculate "(Point(x: 2, y: 3) * 2).x"
    Then the result is "4"

  Scenario: A data type can have fields of another data type
    Given I calculate "data Point { x: Number, y: Number }"
    And I calculate "data Line { a: Point, b: Point }"
    And I calculate "l = Line(a: Point(x: 1, y: 2), b: Point(x: 3, y: 4))"
    When I calculate "l.b.y"
    Then the result is "4"
    When I calculate "l == Line(a: Point(x: 1, y: 2), b: Point(x: 3, y: 4))"
    Then the result is "1"

  Scenario: A nested field rejects the wrong type
    Given I calculate "data Point { x: Number, y: Number }"
    And I calculate "data Line { a: Point, b: Point }"
    When I calculate "Line(a: 5, b: Point(x: 3, y: 4))"
    Then the calculation fails mentioning "is a Point"

  Scenario: Nested records survive save and reopen
    Given I calculate "data Point { x: Number, y: Number }"
    And I calculate "data Line { a: Point, b: Point }"
    And I calculate "seg = Line(a: Point(x: 1, y: 1), b: Point(x: 4, y: 5))"
    When the workbook is saved and reopened
    And I calculate "seg.b.x - seg.a.x"
    Then the result is "3"

  Scenario: A field can be a list of scalars
    Given I calculate "data Tags { items: [String] }"
    When I calculate "Tags(items: ["a", "b", "c"])"
    Then the result is "Tags(items: ["a", "b", "c"])"
    When I calculate "len(Tags(items: ["a", "b"]).items)"
    Then the result is "2"

  Scenario: A field can be a list of records, nested freely
    Given I calculate "data Point { x: Number, y: Number }"
    And I calculate "data Path { points: [Point] }"
    And I calculate "p = Path(points: [Point(x: 1, y: 2), Point(x: 3, y: 4)])"
    When I calculate "p.points[1].y"
    Then the result is "4"
    When I calculate "len(p.points)"
    Then the result is "2"

  Scenario: A field can be a nested list
    Given I calculate "data Grid { rows: [[Number]] }"
    When I calculate "Grid(rows: [[1, 2], [3, 4]]).rows[1][0]"
    Then the result is "3"

  Scenario: A field can be a string-keyed map
    Given I calculate "data Config { opts: {String: Number} }"
    When I calculate "Config(opts: {a: 1, b: 2}).opts.b"
    Then the result is "2"

  Scenario Outline: List and map fields validate their elements
    Given I calculate "data Tags { items: [String] }"
    And I calculate "data Config { opts: {String: Number} }"
    When I calculate "<expr>"
    Then the calculation fails mentioning "<message>"

    Examples:
      | expr                   | message       |
      | Tags(items: [1, 2])    | is a String   |
      | Tags(items: "x")       | is a [String] |
      | Config(opts: {a: "x"}) | is a Number   |

  Scenario: List and map fields survive save and reopen
    Given I calculate "data Point { x: Number, y: Number }"
    And I calculate "data Path { points: [Point] }"
    And I calculate "route = Path(points: [Point(x: 1, y: 1), Point(x: 5, y: 9)])"
    When the workbook is saved and reopened
    And I calculate "route.points[1].x"
    Then the result is "5"
