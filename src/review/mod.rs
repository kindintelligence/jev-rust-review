//! The review pipeline. `diagnostics` collects tool facts about a diff,
//! `evaluate` triages it with Jev, and `verify` checks candidate findings. Both return plain data that the MCP layer serialises.

mod cargo_facts;
mod diagnostics;
mod evaluate;
mod related;
mod types;
mod verify;

pub use cargo_facts::{compare_lockfiles, compare_manifests};
pub use diagnostics::*;
pub use evaluate::*;
pub use types::*;
pub use verify::*;
