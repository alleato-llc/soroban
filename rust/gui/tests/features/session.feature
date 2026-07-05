Feature: The Rust app session — calculator and sheet, headless

  Driven directly against the UI-free `Session` (no iced, no rendering) — the
  Rust counterpart to the Swift SorobanSessionTests, but a fast `cargo test`.
  This suite is Rust-only (it exercises what the port supports today); the
  cross-ecosystem parity oracle is spec/anzan, run by the engine's gherkin suite.

  Background:
    Given a fresh session

  # ---- Calculator (the log) ----

  Scenario: A fresh session starts on Sheet 1
    Then the active sheet is named "Sheet 1"

  Scenario: A calculation lands in the log at full precision
    When I enter "0.1 + 0.2"
    Then the result is "0.3"

  Scenario: ans carries the last result across lines
    When I enter "6 * 7"
    And I enter "ans + 1"
    Then the result is "43"

  Scenario: Defining a function is reported, then it is callable
    When I enter "double(x) = x * 2"
    Then the log defines a function "double(x)"
    When I enter "double(21)"
    Then the result is "42"

  Scenario: The :mode command switches the log dialect
    When I enter "5 ^ 3"
    Then the result is "125"
    When I enter ":mode programmer"
    Then the mode is "programmer"
    When I enter "5 ^ 3"
    Then the result is "6"
    When I enter "17 % 5"
    Then the result is "2"
    When I enter ":mode normal"
    Then the mode is "normal"
    When I enter "5 ^ 3"
    Then the result is "125"

  Scenario: History reflects the calculation log from a log-line expression
    When I enter "10 + 5"
    And I enter "42"
    And I enter "len(History)"
    Then the result is "2"
    When I enter "first(History).value"
    Then the result is "15"
    When I enter "History[1].value"
    Then the result is "42"

  Scenario: A bad expression fails with a message
    When I enter "1 / 0"
    Then the last line fails mentioning "division by zero"

  Scenario: A comment line is recorded as a note
    When I enter "# just a note"
    Then the last line is a note "just a note"

  Scenario: Up and down recall the input history
    When I enter "10 + 1"
    And I enter "20 + 2"
    And I recall the previous input
    Then the input line holds "20 + 2"
    When I recall the previous input
    Then the input line holds "10 + 1"
    When I recall the next input
    Then the input line holds "20 + 2"

  Scenario: The reference documents a built-in function
    Then the reference for "pmt" documents it

  Scenario: The inspector lists log-defined names
    When I enter "rate = 0.0825"
    And I enter "double(x) = x * 2"
    And I enter "data Point { x: Number, y: Number }"
    Then the inspector lists the variable "rate"
    And the inspector lists the function "double(x)"
    And the inspector lists the data type "Point"

  Scenario: The inspector also lists sheet-scoped definitions
    When I set cell A:1 to "taxRate = 0.0825"
    And I set cell A:2 to "tax(x) = x * taxRate"
    Then the inspector lists the variable "taxRate"
    And the inspector lists the function "tax(x)"

  # ---- Sheet (the grid) ----

  Scenario: A number cell shows its value
    When I set cell A:1 to "1200"
    Then cell A:1 shows "1200"

  Scenario: A formula reads another cell
    When I set cell A:1 to "1200"
    And I set cell A:2 to "=A:1 * 2"
    Then cell A:2 shows "2400"

  Scenario: A label stays text; a bad formula shows an error
    When I set cell A:1 to "Revenue"
    Then cell A:1 shows "Revenue"
    When I set cell B:1 to "=1 / 0"
    Then cell B:1 shows an error mentioning "division by zero"

  Scenario: The log and the grid share one variable space
    When I enter "rate = 0.1"
    And I set cell A:1 to "=100 * rate"
    Then cell A:1 shows "10"

  Scenario: A cell can be formatted, display-only, and it round-trips
    When I set cell A:1 to "0.0825"
    And I make cell A:1 bold
    And I format cell A:1 as percent
    Then cell A:1 is bold
    And cell A:1 is formatted as percent
    And cell A:1 shows "0.0825"

  Scenario: Renaming a named cell rewrites the formulas that reference it
    When I set cell A:1 to "5"
    And I name cell A:1 "Rate"
    And I set cell B:1 to "=100 * 'Rate'"
    Then cell B:1 shows "500"
    When I rename cell A:1 to "Factor"
    Then cell B:1 contains "=100 * 'Factor'"
    And cell B:1 shows "500"
    When I undo
    Then cell B:1 contains "=100 * 'Rate'"

  Scenario: Undo reverts a formatting change and naming a cell
    When I set cell A:1 to "5"
    And I make cell A:1 bold
    And I name cell A:1 "Rate"
    Then cell A:1 is bold
    When I undo
    Then cell A:1 is not named
    When I undo
    Then cell A:1 is not bold

  Scenario: Control cells are enumerated for the sheet
    When I set cell A:1 to "n = stepper(5, 1, 20)"
    And I set cell B:2 to "flag = checkbox(true)"
    Then the sheet has a control in A:1
    And the sheet has a control in B:2

  Scenario: Starting a new workbook clears the sheet and the log's variables
    When I enter "taxRate = 5"
    And I set cell A:1 to "42"
    And I start a new workbook
    Then cell A:1 shows ""
    When I enter "taxRate"
    Then the last line fails mentioning "unknown variable"

  # ---- Undo / redo ----

  Scenario: Undo and redo walk a cell edit
    When I set cell A:1 to "5"
    And I set cell A:1 to "9"
    Then cell A:1 shows "9"
    When I undo
    Then cell A:1 shows "5"
    When I redo
    Then cell A:1 shows "9"

  # ---- Point mode (Excel-style reference insertion while editing) ----

  Scenario: Clicking a cell after an operator inserts its reference
    When I begin editing cell A:1
    And I type "=B:1 +" into the editor
    And I click cell C:1
    Then the editor holds "=B:1 +C:1"

  Scenario: Clicking a cell right after "=" inserts its reference
    When I begin editing cell A:1
    And I type "=" into the editor
    And I click cell B:2
    Then the editor holds "=B:2"

  Scenario: Point mode inserts a cell's name, not its address, when it has one
    When I set cell B:1 to "5"
    And I name cell B:1 "Rate"
    And I begin editing cell A:1
    And I type "=100 * " into the editor
    And I click cell B:1
    Then the editor holds "=100 * 'Rate'"

  Scenario: Clicking away from a complete value commits instead of inserting
    When I begin editing cell A:1
    And I type "42" into the editor
    And I click cell C:1
    Then the editor is closed
    And cell A:1 shows "42"

  Scenario: Re-clicking replaces the just-inserted reference
    When I begin editing cell A:1
    And I type "=" into the editor
    And I click cell B:1
    Then the editor holds "=B:1"
    When I click cell C:1
    Then the editor holds "=C:1"

  Scenario: Shift-clicking after an insert extends the reference into a range
    When I begin editing cell A:1
    And I type "=sum(" into the editor
    And I click cell B:1
    Then the editor holds "=sum(B:1"
    When I shift-click cell B:4
    Then the editor holds "=sum(B:1..B:4"

  Scenario: A range extension uses addresses even when the first corner is named
    When I set cell B:1 to "5"
    And I name cell B:1 "Rate"
    And I begin editing cell A:1
    And I type "=sum(" into the editor
    And I click cell B:1
    Then the editor holds "=sum('Rate'"
    When I shift-click cell B:4
    Then the editor holds "=sum(B:1..B:4"

  Scenario: Typing after an insert ends the replace window, so the next click appends
    When I begin editing cell A:1
    And I type "=" into the editor
    And I click cell B:1
    Then the editor holds "=B:1"
    When I type "=B:1 +" into the editor
    And I click cell C:1
    Then the editor holds "=B:1 +C:1"

  # ---- Controls ----

  Scenario: Toggling a checkbox rewrites its own cell
    When I set cell A:1 to "flag = checkbox(true)"
    Then cell A:1 shows "checked"
    When I toggle the checkbox in A:1
    Then cell A:1 shows "unchecked"

  Scenario: A slider commits its value into the cell text
    When I set cell A:1 to "rate = slider(0.08, 0, 0.2)"
    And I set the slider in A:1 to "0.12"
    Then cell A:1 shows "slider:0.12"

  Scenario: Stepping a stepper walks its value by the step
    When I set cell A:1 to "n = stepper(5, 1, 20)"
    Then cell A:1 shows "slider:5"
    When I step A:1 up
    Then cell A:1 shows "slider:6"
    When I step A:1 down
    Then cell A:1 shows "slider:5"

  Scenario: Picking a dropdown option rewrites the cell to that option
    When I set cell A:1 to "choice = dropdown(1, [1, 2, 3])"
    Then cell A:1 shows "1"
    When I pick option 2 in the dropdown in A:1
    Then cell A:1 shows "3"

  # ---- Copy / paste (TSV) ----

  Scenario: Copy then paste moves cell contents
    When I set cell A:1 to "5"
    And I set cell A:2 to "6"
    And I copy A:1 through A:2
    And I paste at C:1
    Then cell C:1 shows "5"
    And cell C:2 shows "6"

  Scenario: Copy and paste carry a rectangular block across rows and columns
    When I set cell A:1 to "1"
    And I set cell B:1 to "2"
    And I set cell A:2 to "3"
    And I set cell B:2 to "4"
    And I copy A:1 through B:2
    And I paste at D:1
    Then cell D:1 shows "1"
    And cell E:1 shows "2"
    And cell D:2 shows "3"
    And cell E:2 shows "4"

  Scenario: Cutting a cell clears it, and paste drops it elsewhere
    When I set cell A:1 to "7"
    And I cut A:1 through A:1
    And I paste at C:1
    Then cell A:1 shows ""
    And cell C:1 shows "7"

  Scenario: Pasting a block past the last column clips the overflow
    When I set cell A:1 to "8"
    And I set cell B:1 to "9"
    And I copy A:1 through B:1
    And I paste at Z:1
    Then cell Z:1 shows "8"

  # ---- Binary bit editor ----

  Scenario: The bit editor reflects and edits the last integer result
    When I enter "5"
    And I open the bit editor
    Then the bit editor is editable
    When I flip bit 1
    Then the bit editor value is "7"
    When I use the bit editor value
    Then the input line holds "7"

  Scenario: The bit editor is unavailable for a non-integer result
    When I enter "0.5"
    And I open the bit editor
    Then the bit editor is not editable

  Scenario: The bit grid is LSB-first, so flip indices line up with the display
    When I enter "5"
    And I open the bit editor
    # 5 = 0b101 → bit 0 set, bit 1 clear, bit 2 set
    Then bit 0 of the editor is set
    And bit 1 of the editor is clear
    And bit 2 of the editor is set
    When I flip bit 0
    Then bit 0 of the editor is clear
    And the bit editor value is "4"

  Scenario: The bit editor reports the value in hex
    When I enter "500"
    And I open the bit editor
    Then the bit editor reads hex "0x1F4"

  # ---- Width picker ----

  Scenario: A plain integer opens at the default 32-bit width and can be re-widened
    When I enter "5"
    And I open the bit editor
    Then the bit editor width is 32
    When I set the bit editor width to 8
    Then the bit editor width is 8
    When I set the bit editor width to 64
    Then the bit editor width is 64

  Scenario: A width too small to hold the value is disabled
    When I enter "5000"
    And I open the bit editor
    # 5000 needs 13 bits, so 8 can't hold it but 16 can
    Then the width 8 is disabled
    And the width 16 is offered

  Scenario: A fixed-width integer is locked to its own width
    When I enter "Int8(5)"
    And I open the bit editor
    Then the bit editor width is 8
    # A fixed type edits only at its declared width — no picker.
    And the width picker is empty

  # ---- Bit-field formats ----

  Scenario: Applying the Unix permissions preset decodes the value into fields
    When I enter "500"
    And I open the bit editor
    And I apply the "Unix permissions" bit format
    Then the active bit format is "Unix permissions"
    # 500 = 0o764 → owner rwx, group rw-, other r--
    And the bit format has a field "owner" reading "rwx"
    And the bit format has a field "group" reading "rw-"
    And the bit format has a field "other" reading "r--"
    And the bit format field "owner" sits at bit 6 for 3 bits
    And the bit format field "other" sits at bit 0 for 3 bits

  Scenario: The format picker lists the built-in presets
    When I enter "5"
    And I open the bit editor
    Then the bit format picker offers "Unix permissions"
    And the bit format picker offers "IPv4 address"
    And the bit format picker offers "IEEE 754 float"

  Scenario: Applying a format wider than the register bumps the width to fit
    When I enter "5"
    And I open the bit editor
    And I set the bit editor width to 8
    And I apply the "IPv4 address" bit format
    # IPv4 is 32 bits, so the 8-bit register widens to 32
    Then the bit editor width is 32

  Scenario: Applying an unknown format leaves the register plain
    When I enter "5"
    And I open the bit editor
    And I apply the "Nonexistent format" bit format
    Then there is no active bit format

  Scenario: Clearing the format returns to a plain register
    When I enter "500"
    And I open the bit editor
    And I apply the "Unix permissions" bit format
    Then the active bit format is "Unix permissions"
    When I clear the bit format
    Then there is no active bit format

  # ---- Bit-field editing (slice 2) ----

  Scenario: The field kinds are classified for the shell's editors
    When I enter "0"
    And I open the bit editor
    And I apply the "DNS header flags" bit format
    Then the bit format field "QR" has kind flags
    And the bit format field "Opcode" has kind enum
    And the bit format field "Z" has kind reserved
    And the bit format field "RCODE" has kind enum

  Scenario: Typing a numeric field's value updates the register
    When I enter "0"
    And I open the bit editor
    And I apply the "IPv4 address" bit format
    And I set bit format field "octet4" to "42"
    Then the bit format has a field "octet4" reading "42"
    When I set bit format field "octet1" to "192"
    Then the bit format has a field "octet1" reading "192"
    # 192.0.0.42 packs to (192<<24) | 42 = 3221225514
    And the bit editor value is "3221225514"

  Scenario: A hex numeric field reads and writes in its base
    When I enter "0"
    And I open the bit editor
    And I apply the "MAC address" bit format
    And I set bit format field "nic3" to "0xff"
    Then the bit format has a field "nic3" reading "0xff"

  Scenario: Picking an enum field's option selects and decodes it
    When I enter "0"
    And I open the bit editor
    And I apply the "DNS header flags" bit format
    Then the bit format field "Opcode" offers "IQUERY"
    When I set bit format field "Opcode" to "1"
    Then the bit format has a field "Opcode" reading "IQUERY"
    And the bit format field "Opcode" is selected as index 1

  Scenario: A flags field exposes its bits high-to-low with their state
    When I enter "500"
    And I open the bit editor
    And I apply the "Unix permissions" bit format
    # group = 6 = rw- → r set, w set, x clear
    Then the bit format field "group" flag "r" is set
    And the bit format field "group" flag "w" is set
    And the bit format field "group" flag "x" is clear

  Scenario: Flipping a flag bit toggles just that flag
    When I enter "500"
    And I open the bit editor
    And I apply the "Unix permissions" bit format
    Then the bit format field "other" flag "w" is clear
    # other 'w' is bit 1 (other = bits 2:0, w is the middle)
    When I flip bit 1
    Then the bit format field "other" flag "w" is set

  Scenario: A field value out of the format's range is rejected
    When I enter "0"
    And I open the bit editor
    And I apply the "IPv4 address" bit format
    Then setting bit format field "nope" to "1" is rejected

  # ---- Building & saving custom formats (slice 3) ----

  Scenario: Building a custom format and applying it
    When I enter "255"
    And I open the bit editor
    And I begin building a new bit format
    And I add a numeric field "hi" of 4 bits
    And I add a numeric field "lo" of 4 bits
    Then the builder has 2 fields
    When I apply the built format
    Then the active bit format is "Custom"
    # first-added field takes the high bits (fields pack high→low)
    And the bit format field "hi" sits at bit 4 for 4 bits
    And the bit format field "lo" sits at bit 0 for 4 bits

  Scenario: A built format decodes flag and enum fields
    When I enter "0"
    And I open the bit editor
    And I begin building a new bit format
    And I add a flags field "perms" of 3 bits labelled "r, w, x"
    And I add an enum field "mode" of 2 bits labelled "idle, run, halt"
    When I apply the built format
    Then the bit format field "perms" has kind flags
    And the bit format field "mode" has kind enum
    And the bit format field "mode" offers "run"

  Scenario: Saving a custom format lists it and makes it applicable
    When I enter "0"
    And I open the bit editor
    And I begin building a new bit format
    And I add a numeric field "a" of 8 bits
    And I save the format as "myfmt"
    Then the active bit format is "myfmt"
    And the saved formats include "myfmt"
    When I clear the bit format
    And I apply the "myfmt" bit format
    Then the active bit format is "myfmt"
    And the bit format field "a" sits at bit 0 for 8 bits

  Scenario: A saved custom format survives save and reopen
    When I enter "0"
    And I open the bit editor
    And I begin building a new bit format
    And I add a numeric field "octet" of 8 bits labelled ""
    And I save the format as "mymac"
    And I save and reopen the workbook
    Then the saved formats include "mymac"
    When I open the bit editor
    And I apply the "mymac" bit format
    Then the active bit format is "mymac"

  Scenario: Editing the current format seeds the builder from its fields
    When I enter "500"
    And I open the bit editor
    And I apply the "Unix permissions" bit format
    And I begin editing the current bit format
    Then the builder has 3 fields

  Scenario: Saving is rejected with no fields built
    When I enter "0"
    And I open the bit editor
    And I begin building a new bit format
    Then saving the format as "empty" is rejected

  Scenario: Deleting a saved format removes it
    When I enter "0"
    And I open the bit editor
    And I begin building a new bit format
    And I add a numeric field "a" of 8 bits
    And I save the format as "tmp"
    Then the saved formats include "tmp"
    When I delete the saved format "tmp"
    Then the saved formats exclude "tmp"

  # ---- Named cells ----

  Scenario: A named cell reads by name from a formula
    When I set cell A:1 to "0.0825"
    And I name cell A:1 "Rate"
    And I set cell B:1 to "=100 * 'Rate'"
    Then cell B:1 shows "8.25"

  Scenario: A named cell appears in the inspector
    When I set cell A:1 to "42"
    And I name cell A:1 "Budget"
    Then the inspector lists the variable "Budget"

  Scenario: A duplicate cell name is rejected
    When I set cell A:1 to "1"
    And I name cell A:1 "Rate"
    And I set cell A:2 to "2"
    Then naming cell A:2 "Rate" is rejected

  # ---- Column widths ----

  Scenario: A column width round-trips
    When I set column A width to "150"
    Then column A width is "150"

  # ---- Workbook round trip ----

  Scenario: Saving and reopening restores cells and variables
    When I enter "rate = 0.25"
    And I set cell A:1 to "=rate * 4"
    And I save and reopen the workbook
    Then cell A:1 shows "1"
