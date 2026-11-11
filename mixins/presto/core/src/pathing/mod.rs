//! Path finding (`baritone.pathing`): costs, the rules for each block, the movements, the A* search, paths,
//! and the executor that walks them.

pub mod calc;
pub mod context;
pub mod executor;
pub mod costs;
pub mod goals;
pub mod helper;
pub mod movement;
pub mod path;
pub mod precomputed;

#[cfg(test)]
pub mod tests;
