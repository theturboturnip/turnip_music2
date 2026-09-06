//! This crate defines and tests the inner workings of turnip_music2.
//! turnip_music2 uses TOML files to define music libraries.
//! For documentation on these files, look in [data_model::user_defined].

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
