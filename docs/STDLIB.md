# A standard library for Anzan — design note (deferred)

> **Status: NOT planned, captured for later.** This records the evaluation of
> shipping built-in *namespace modules written in Anzan* (e.g. a `Net`
> subnetting toolkit) loaded into every session's prelude. The conclusion: it's
> feasible and the module system ([MODULES.md](MODULES.md)) is the enabling
> piece, but it trades the calculator's lean, exact identity for convenience and
> introduces a real persistence/compatibility surface. **Recommendation: ship
> recipes as documentation + insertable examples + template workbooks first; gate
> a true preloaded stdlib behind demonstrated demand and a fuller spec.**

## The opportunity

Now that namespaces, imports, and `::` exist, a curated module like `Net` could
be authored *in Anzan* and loaded into every fresh `Calculator` — shared by the
app and the CLI, since `Calculator` init is the natural seam. Reachable as
`Net::network`, `import`-able, and self-documenting from `#` comments. It would
dogfood the language instead of adding Swift builtins, and it respects the
design-rules gate (compositions stay out of the engine registry — see
[ANZAN.md](ANZAN.md) "Design rules"). The [IPv4/IPv6 subnetting toolkit in
PROGRAMMER.md](PROGRAMMER.md) is the archetype.

## The hard problems (all about persistence & identity)

1. **Replay collisions.** Opening a workbook replays user namespace sources;
   a preloaded stdlib would also register its members. If preload used the normal
   `evaluate` path it would (a) get recorded into `namespaceSources` and persist
   into the workbook, then (b) collide with itself on reopen
   (`'Net::network' is already a function`). A stdlib needs a **separate preload
   path** that registers without recording or persisting.
2. **Override semantics.** A user's own `namespace Net { … }` must cleanly win
   over (or be rejected against) the shipped one — but re-registering a namespace
   member currently throws. We'd need "soft" stdlib registration that user
   definitions replace.
3. **Compatibility surface.** A workbook that *uses* `Net::network` without
   defining it depends on the stdlib version present. Open it on a build without
   that stdlib (or with a renamed function) and it breaks — silently degrading
   references to text/errors. That makes the stdlib an **API with a stability
   promise** and demands version stamping.
4. **Discoverability.** Stdlib-as-source functions are "user functions," so they
   wouldn't appear in the ⌘/ reference window or carry categories unless that
   window is taught to surface namespace members.

## Philosophy

Soroban's stated identity is lean and exact — "be exact, then stay out of the
way." A standard library is a real fork toward batteries-included. Defensible,
but a *direction*, not a tweak.

## The options ladder (rising commitment)

- **(a) Recipes as docs + insertable examples.** Worked examples in the docs
  (done for subnetting) and an in-app "Recipes" entry that drops a whole
  namespace into the log. Zero global state, zero compatibility surface; the user
  owns the code. *The recommended next step if more reach is wanted.*
- **(b) Template workbooks.** Ship a sample `.soroban` (e.g. "Networking") with
  the namespaces predefined; open as a starting point. Discoverable, still no
  hidden global state.
- **(c) A true preloaded stdlib.** Only if (a)/(b) show demand. A real project:
  a reserved stdlib-namespace set, a non-recording preload path, soft/override
  registration, workbook version stamping, and reference-window integration —
  each of the four problems above, solved deliberately. Deserves its own full
  spec before any code (the way this doc precedes it).

## Recommendation

Ship (a) and (b); do **not** auto-load a stdlib yet. Revisit (c) with a fuller
spec only if recipes prove popular enough to justify the persistence and
compatibility machinery.
