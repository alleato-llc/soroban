/// SorobanEngine hosts the Anzan language (grid + persistence around it) —
/// re-export it so `import SorobanEngine` keeps giving the app and tests
/// the whole engine, exactly as before the language became its own module.
@_exported import Anzan
