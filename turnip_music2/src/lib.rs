#![allow(unused)] // for now

/// CLI-facing modules, agnostic to filesystem and warning mechanisms
pub mod cli;
pub mod data_model;
pub mod fs;
pub mod resolver;
pub mod scanner;
/// Types and convenience functions for editing TOML
pub mod toml;
pub mod util;
pub mod warning;

#[cfg(test)]
mod tests;
