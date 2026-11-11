//! `ActionCosts`: what moving costs, in ticks.

use std::sync::OnceLock;

pub const WALK_ONE_BLOCK_COST: f64 = 20.0 / 4.317;
pub const WALK_ONE_IN_WATER_COST: f64 = 20.0 / 2.2;
/// 0.4 in BlockSoulSand but effectively about half.
pub const WALK_ONE_OVER_SOUL_SAND_COST: f64 = WALK_ONE_BLOCK_COST * 2.0;
pub const LADDER_UP_ONE_COST: f64 = 20.0 / 2.35;
pub const LADDER_DOWN_ONE_COST: f64 = 20.0 / 3.0;
pub const SNEAK_ONE_BLOCK_COST: f64 = 20.0 / 1.3;
pub const SPRINT_ONE_BLOCK_COST: f64 = 20.0 / 5.612;
pub const SPRINT_MULTIPLIER: f64 = SPRINT_ONE_BLOCK_COST / WALK_ONE_BLOCK_COST;
/// Walking off an edge: 0.5 to the edge, then 0.3 to start falling.
pub const WALK_OFF_BLOCK_COST: f64 = WALK_ONE_BLOCK_COST * 0.8;
/// The rest of the way to be centred on the new block.
pub const CENTER_AFTER_FALL_COST: f64 = WALK_ONE_BLOCK_COST - WALK_OFF_BLOCK_COST;
/// Not `f64::MAX`: costs are added together, and this must not overflow to anything odd.
pub const COST_INF: f64 = 1_000_000.0;

fn velocity(ticks: i32) -> f64 {
    (0.98f64.powi(ticks) - 1.0) * -3.92
}

/// `ActionCosts.distanceToTicks`.
pub fn distance_to_ticks(distance: f64) -> f64 {
    if distance == 0.0 {
        return 0.0;
    }
    let mut tmp = distance;
    let mut tick_count = 0;
    loop {
        let fall = velocity(tick_count);
        if tmp <= fall {
            return tick_count as f64 + tmp / fall;
        }
        tmp -= fall;
        tick_count += 1;
    }
}

/// `FALL_N_BLOCKS_COST[n]`: the ticks a fall of `n` blocks takes (n up to 4096).
pub fn fall_n_blocks_cost(n: usize) -> f64 {
    static TABLE: OnceLock<Box<[f64]>> = OnceLock::new();
    let table = TABLE.get_or_init(|| (0..4097).map(|i| distance_to_ticks(i as f64)).collect());
    table[n.min(4096)]
}

pub fn fall_1_25_blocks_cost() -> f64 {
    static V: OnceLock<f64> = OnceLock::new();
    *V.get_or_init(|| distance_to_ticks(1.25))
}

pub fn fall_0_25_blocks_cost() -> f64 {
    static V: OnceLock<f64> = OnceLock::new();
    *V.get_or_init(|| distance_to_ticks(0.25))
}

/// A jump's rise of one block: the 0..1.25 climb less the 1..1.25 part (a parabola is symmetric, so that
/// part takes as long as falling from 1.25 back to 1).
pub fn jump_one_block_cost() -> f64 {
    fall_1_25_blocks_cost() - fall_0_25_blocks_cost()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn costs_match_baritone() {
        assert!((WALK_ONE_BLOCK_COST - 4.633).abs() < 1e-3);
        assert!((SPRINT_ONE_BLOCK_COST - 3.564).abs() < 1e-3);
        assert!((WALK_OFF_BLOCK_COST - 3.706).abs() < 1e-3);
        assert!((CENTER_AFTER_FALL_COST - 0.927).abs() < 1e-3);
        // Falls: one block takes about 5.6 ticks, a jump's rise about 2.8.
        assert!((fall_n_blocks_cost(1) - 5.6).abs() < 0.2);
        assert!(fall_n_blocks_cost(10) > fall_n_blocks_cost(9));
        assert!(jump_one_block_cost() > 2.0 && jump_one_block_cost() < 4.0);
    }
}
