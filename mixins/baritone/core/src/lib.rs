//! Baritone, the Minecraft pathfinder bot, ported to Rust for Horizon (Minecraft: Java Edition 26.3 in Rust).
//!
//! This crate is Baritone itself, with no tie to any game: path finding (`pathing`), the cache of chunks seen
//! before (`cache`), what Baritone does with a path (`behavior`, `process`), its commands and settings. The game
//! hands it a snapshot each tick ([`player::GameState`]) and applies what it asks for ([`player::Controls`]);
//! Horizon's side of that lives in the mod's glue (`mixins/baritone/glue.rs`).
//!
//! The module layout follows Baritone's Java packages, and items keep their Java names in their docs, so the
//! two read side by side.

pub mod baritone;
pub mod behavior;
pub mod bot;
pub mod cache;
pub mod command;
pub mod ctx;
pub mod math;
pub mod pathing;
pub mod player;
pub mod process;
pub mod settings;
#[cfg(test)]
mod sim;
pub mod utils;
pub mod world;

pub use baritone::Baritone;
pub use math::{Aabb, BlockPos, Direction, Rotation, Vec3};
pub use settings::Settings;
