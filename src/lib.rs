//! jev-rust-review: an MCP server that uses TypeSafe Jev as a triage and
//! verification layer for Rust code review.
//!
//! stdout belongs to the MCP protocol; everything else logs to stderr.
#![deny(clippy::print_stdout)]

pub mod cargo_tools;
pub mod config;
pub mod context;
pub mod diff;
pub mod error;
pub mod facts;
pub mod git;
pub mod jev;
pub mod mcp;
pub mod questions;
pub mod redact;
pub mod review;
pub mod rust_project;
