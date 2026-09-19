//! The review pipeline. `evaluate` triages a diff with Jev; `verify` checks
//! candidate findings. Both return plain data that the MCP layer serialises.

mod cargo_facts;
mod evaluate;
mod types;
mod verify;

pub use cargo_facts::{compare_lockfiles, compare_manifests};
pub use evaluate::*;
pub use types::*;
pub use verify::*;
