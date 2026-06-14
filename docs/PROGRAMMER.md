# Programmer mode & the bit editor

> **Status: implemented.** Programmer-mode dialect (engine + CLI `:mode` + app
> picker), the binary bit editor overlay, bit-field formats (numeric / flags /
> enum, color-coded), the typed `Bits` module persistence, and the visual format
> builder are all landed and tested (`BinaryViewTests`, `anzan.feature`,
> `modules.feature`). This is the bird's-eye view; the precise specs live in
> [MODES.md](MODES.md) (the dialect), [FIXED-WIDTH.md](FIXED-WIDTH.md) (the
> `Int`/`UInt` types), and [MODULES.md](MODULES.md) (the `Bits` module).

Soroban has a "programmer" face: a C-style operator dialect for the log and CLI,
and — in the app — a macOS-Calculator-style **binary bit editor** that turns a
number into a clickable register, optionally labeled by a reusable **bit-field
format**. None of it changes the math: every value is still an exact integer,
and a stored formula means the same thing in every mode.

## Programmer mode (the dialect)

Flip the log to **Programmer mode** (the input-bar affordance, the Settings
picker, or `:mode programmer` in the CLI) and the overloaded glyphs read like C:

| glyph | normal | programmer |
|-------|--------|------------|
| `^`   | power  | bitwise XOR |
| `&` `\|` | (reserved) | bitwise AND / OR |
| `<<` `>>` | (reserved) | left / right shift (`>>` negates the count) |
| `~`   | (reserved) | bitwise NOT (prefix) |
| `%`   | percent | modulo |

Precedence shifts to Python's (a bitwise band sits between comparison and
additive; power becomes `pow()`). Crucially this is a **display dialect, not a
semantic switch** — the stored program is always canonical, so it can never mean
two things. An out-of-mode glyph is a loud error, not a silent reinterpretation.
Full rules: [MODES.md](MODES.md).

Radix literals work in any mode: `0xFF`, `0b1010` (arbitrary width via the exact
integer engine). Display radix is a per-cell **format** (`hex`/`binary`) — again
display-only, never a change of meaning.

## The binary bit editor

In the app, Programmer mode reveals a **bit editor** overlay (toggle with
⌥⌘B, or the View menu). It shows the current answer as a grid of bits:

- A **plain non-negative integer** edits as an unsigned register at a width you
  pick (8 / 16 / 32 / 64 / 128 / 256, auto-bumped to fit). Widths too narrow for
  the value are grayed out.
- A **fixed-width integer** (`Int32(…)`, `UInt8(…)`) edits at its own declared
  width and signedness, in full two's-complement. See [FIXED-WIDTH.md](FIXED-WIDTH.md).
- A negative plain number, a decimal, or a value over 256 bits isn't editable —
  the overlay explains why (wrap a negative in a signed `Int…` to edit its bits).

Clicking a bit flips it and stages the change **live** — no log spam. The
decimal and hex readouts update as you go; **double-click** either to insert it
into the input line. **Use** drops the staged value into the expression (a `0b…`
literal for a plain integer, the typed constructor for an `Int…`) so you fold it
into a larger expression and submit when ready; the ↺ button resets to the
answer.

**ans-prefix continuation** (SpeedCrunch-style): typing a leading binary operator
on an otherwise-empty input line prefixes `ans` automatically — `*2` becomes
`ans*2`. `%` and the bitwise glyphs lead only in Programmer mode.

## Bit-field formats

A raw register is bits; a **format** labels named bit ranges so the register
reads as a diagram — a permission mask, a packed struct, a protocol header.
Apply one from **Format ▾** (presets like Unix permissions, TCP flags, RGB565),
and each field becomes a captioned, color-coded band. A field is one of three
kinds:

- **numeric** — a plain value you can type into (clamped to the field's width).
- **flags** — one name per bit, high→low; the readout decodes positionally
  (`rwx`, `r-x`), or lists the set names for multi-character flags (`ACK SYN`).
- **enum** — the field's unsigned value indexes a label list; value `2` of
  `["idle", "run", "halt", "max"]` reads `halt`. Enum fields render as a labeled
  picker.

Fields pack contiguously into the low bits, listed high→low; any higher bits are
shown as "unused."

A **numeric** field also carries a display **base** — decimal (the default) or
hex — so an octet can read `0x1b` instead of `27`. It's presentation only (the
bits are unchanged), and input is parsed in that base, though an explicit `0x`
prefix always wins, so a hex field accepts `1b` or `0x1b`. Flags and enum fields
ignore base (they show names).

## The `Bits` module (how formats persist)

Saving a format writes a typed record to the calculation log, so it lives in the
workbook and survives reopen:

```
namespace Bits { data BitField { name: String, bits: Number, kind: String,
                                  flags: [String], values: [String], color: String };
                 data BitFormat { fields: [BitField] } }

perms = Bits::BitFormat(fields: [
    Bits::BitField(name: "owner", bits: 3, kind: "flags", flags: ["r","w","x"], values: [], color: "blue"),
    Bits::BitField(name: "mode",  bits: 2, kind: "enum",  flags: [], values: ["idle","run","halt","max"], color: "green") ])
```

The schema is defined once per workbook. Because a format is just an ordinary
typed variable, it's fully manipulable from the language — `perms.fields`, edit
a `BitField`, re-run — and it never pollutes the global namespace (it lives under
`Bits::`). The `Bits` module is the payoff feature of the broader module system;
see [MODULES.md](MODULES.md).

## The visual builder

You don't have to write that record by hand. **Format ▾ → Build…** opens a
builder:

1. **Click the open bits** to claim a contiguous group (clicking the *j*-th open
   cell claims a *j*-bit group; click the same edge again to clear).
2. **Detail it** in the row that appears: a name, the kind (numeric / flags /
   enum), and the labels (flag names or enum values), plus a **color** swatch.
3. **Add field** — it becomes a colored band; repeat for the next group.
4. **Recolor** any committed field from the color dot on its band; **✕** removes
   it.
5. **Save** under a name (persists the typed `Bits::BitFormat` and applies it) or
   **Apply** (uses it now without saving). Entering build mode seeds from the
   active format, so you can tweak an existing one.

Colors are presentational — they round-trip through the saved format as a
palette name (blue · green · orange · purple · pink · teal) and adapt to the
active theme.

## Worked example: IPv4 subnetting

The bit editor's **IPv4 address** preset splits a 32-bit register into four
8-bit octets, so you can type an address as a plain integer (or `0x…`) and read
or edit it dotted-quad style. The arithmetic side — masks, network, broadcast,
host counts — is a few lines of Anzan, and it's a tidy showcase of three
features at once: **namespaces** keep generic names like `mask`/`network` out of
the global scope, **fixed-width integers** give a real 32-bit register, and the
**bitwise builtins** do the work.

```
namespace Net {
    ip(a,b,c,d)        = UInt32(a)*16777216 + UInt32(b)*65536 + UInt32(c)*256 + UInt32(d);
    mask(p)            = UInt32(2^32 - 2^(32 - p));
    wildcard(p)        = bitNot(mask(p));
    network(addr, p)   = bitAnd(addr, mask(p));
    broadcast(addr, p) = bitOr(addr, wildcard(p));
    hosts(p)           = 2^(32 - p) - 2;
    octet(addr, i)     = bitAnd(bitShift(addr, -8 * (3 - i)), 255);
    dotted(addr)       = "" + octet(addr,0) + "." + octet(addr,1) + "." + octet(addr,2) + "." + octet(addr,3)
}
```

```
addr = Net::ip(192, 168, 1, 130)
Net::dotted(Net::network(addr, 24))      # 192.168.1.0
Net::dotted(Net::broadcast(addr, 24))    # 192.168.1.255
Net::hosts(24)                           # 254
Net::dotted(Net::network(addr, 26))      # 192.168.1.128
Net::hosts(26)                           # 62
```

Note the mask is computed **arithmetically** (`2^32 - 2^(32-p)`), not by
left-shifting all-ones: fixed-width integers are *checked, not modular*, so
`bitShift(UInt32(4294967295), 8)` would overflow 32 bits and error rather than
silently wrap — the exactness ethos, right down at the bit level. The `bitAnd` /
`bitOr` / `bitNot` / `bitShift` builtins are plain functions (the
function-spelling of Programmer mode's `& | ~ <<`), so this module works in any
mode and persists in the workbook like any other.

### IPv6 — same shape, 128 bits

An IPv6 address is 128 bits, which is exactly a `UInt128`. The toolkit is the
same, scaled up — and host counts stay exact (a `/64` really is 2⁶⁴ addresses):

```
namespace Net6 {
    mask(p)            = UInt128(2^128 - 2^(128 - p));
    network(addr, p)   = bitAnd(addr, mask(p));
    broadcast(addr, p) = bitOr(addr, bitNot(mask(p)));
    hosts(p)           = 2^(128 - p);                       # IPv6: no -2 convention
    hextet(addr, i)    = bitAnd(bitShift(addr, -16 * (7 - i)), 65535)
}

Net6::hosts(64)   # 18446744073709551616  (= 2^64, exact)
```

### MAC — a flat 48-bit address with an OUI/device split

A MAC has no subnet, but it does split: the top 24 bits are the **OUI** (the
vendor block), the low 24 the device. 48 bits isn't an allowed fixed width, so a
MAC lives in a `UInt64`:

```
mac = UInt64(0x001B44113AB7)
oui = bitAnd(bitShift(mac, -24), 0xFFFFFF)   # 0x001B44  (the vendor)
nic = bitAnd(mac, 0xFFFFFF)                  # 0x113AB7  (the device)
```

In the bit editor, the built-in **MAC address** and **IPv6 address** presets lay
these out as octet / hextet bands (MAC rounds its register up to 64 bits, since
48 isn't a register width), and they read in **hex** — each numeric field
carries a display base (see below), so the octets show `0x1b` rather than `27`,
matching how MAC and IPv6 are conventionally written.

## In the CLI

The `soroban` CLI is Programmer-mode aware via `:mode programmer` (REPL and
pipe): the same dialect glyphs and the same column-accurate error carets. The
CLI has the language, not the grid — the bit editor and bit-field formats are an
app-only, visual layer over the same engine.
