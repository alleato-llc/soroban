//! The library face of the Soroban desktop app: the UI-free
//! [`session::Session`] — the engine-facing view-model that the iced shell
//! (`src/main.rs`) wraps. It's split into a library target (rather than living
//! only inside the binary) so the headless BDD suite in `tests/session.rs` can
//! drive `Session` directly — no iced, no rendering — the Rust counterpart to
//! the Swift `SorobanSessionTests`.

pub mod session;
