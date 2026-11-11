//! `MovementDiagonal`: one block diagonally, edging around a corner if one side is blocked, and optionally
//! up or down a block.

use super::{Exec, MoveResult, Movement, MovementState, MovementStatus, move_towards};
use crate::behavior::input::Input;
use crate::math::BlockPos;
use crate::pathing::context::CalculationContext;
use crate::pathing::costs::*;
use crate::pathing::helper::*;
use crate::world::flags;

const SQRT_2: f64 = std::f64::consts::SQRT_2;

pub fn cost(ctx: &CalculationContext, x: i32, y: i32, z: i32, dest_x: i32, dest_z: i32, res: &mut MoveResult) {
    if !can_walk_through(ctx, dest_x, y + 1, dest_z) {
        return;
    }
    let dest_into_state = ctx.get(dest_x, y, dest_z);
    let dest_into = ctx.table.get(dest_into_state);
    let mut ascend = false;
    let mut descend = false;
    let mut frost_walker = false;
    let mut sneaking = false;
    let dest_walk_on;
    let from_down = ctx.info(x, y - 1, z);
    if !can_walk_through_state(ctx, dest_x, y, dest_z, dest_into_state) {
        ascend = true;
        if !ctx.allow_diagonal_ascend
            || !can_walk_through(ctx, x, y + 2, z)
            || !can_walk_on_state(ctx, dest_x, y, dest_z, dest_into_state)
            || !can_walk_through(ctx, dest_x, y + 2, dest_z)
        {
            return;
        }
        dest_walk_on = dest_into;
    } else {
        let dest_walk_on_state = ctx.get(dest_x, y - 1, dest_z);
        dest_walk_on = ctx.table.get(dest_walk_on_state);
        let standing_on_a_block = must_be_solid_to_walk_on(ctx, x, y - 1, z, from_down);
        frost_walker = standing_on_a_block && can_use_frost_walker(ctx, dest_walk_on);
        if !frost_walker && !can_walk_on_state(ctx, dest_x, y - 1, dest_z, dest_walk_on_state) {
            descend = true;
            if !ctx.allow_diagonal_descend
                || !can_walk_on(ctx, dest_x, y - 2, dest_z)
                || !can_walk_through_state(ctx, dest_x, y - 1, dest_z, dest_walk_on_state)
            {
                return;
            }
        }
        // After the descend check: jesus can't stop the water freezing, only stop us relying on it.
        frost_walker &= !ctx.assume_walk_on_water;
    }
    let mut multiplier = WALK_ONE_BLOCK_COST;
    // Soul sand under either end slows half of the walk.
    if dest_walk_on.is(flags::SOUL_SAND) {
        multiplier += (WALK_ONE_OVER_SOUL_SAND_COST - WALK_ONE_BLOCK_COST) / 2.0;
    } else if ctx.allow_walk_on_magma_blocks && dest_walk_on.is(flags::MAGMA) {
        multiplier += (SNEAK_ONE_BLOCK_COST - WALK_ONE_BLOCK_COST) / 2.0;
        sneaking = true;
    } else if frost_walker {
        // Frost Walker walks on water without the penalty.
    } else if dest_walk_on.is(flags::LIQUID_BLOCK) && dest_walk_on.is_water() {
        multiplier += ctx.walk_on_water_one_penalty * SQRT_2;
    }
    if is_climbable(from_down) {
        return;
    }
    if from_down.is(flags::SOUL_SAND) {
        multiplier += (WALK_ONE_OVER_SOUL_SAND_COST - WALK_ONE_BLOCK_COST) / 2.0;
    } else if ctx.allow_walk_on_magma_blocks && from_down.is(flags::MAGMA) {
        multiplier += (SNEAK_ONE_BLOCK_COST - WALK_ONE_BLOCK_COST) / 2.0;
        sneaking = true;
    }
    let cutting_over1 = ctx.info(x, y - 1, dest_z);
    if (!ctx.allow_walk_on_magma_blocks && cutting_over1.is(flags::MAGMA)) || is_lava(cutting_over1) {
        return;
    }
    let cutting_over2 = ctx.info(dest_x, y - 1, z);
    // Baritone checks the first corner's magma twice here; kept.
    if (!ctx.allow_walk_on_magma_blocks && cutting_over1.is(flags::MAGMA)) || is_lava(cutting_over2) {
        return;
    }
    let mut water = false;
    let start = ctx.info(x, y, z);
    if is_water(start) || is_water(dest_into) {
        if ascend {
            return;
        }
        // Floating on water: whatever was under us doesn't matter.
        multiplier = ctx.water_walk_speed;
        water = true;
    }
    let pb0_state = ctx.get(x, y, dest_z);
    let pb2_state = ctx.get(dest_x, y, z);
    let pb0 = ctx.table.get(pb0_state);
    let pb2 = ctx.table.get(pb2_state);
    if ascend {
        let a_top = can_walk_through(ctx, x, y + 2, dest_z);
        let a_mid = can_walk_through(ctx, x, y + 1, dest_z);
        let a_low = can_walk_through_state(ctx, x, y, dest_z, pb0_state);
        let b_top = can_walk_through(ctx, dest_x, y + 2, z);
        let b_mid = can_walk_through(ctx, dest_x, y + 1, z);
        let b_low = can_walk_through_state(ctx, dest_x, y, z, pb2_state);
        if (!(a_top && a_mid && a_low) && !(b_top && b_mid && b_low)) // no option
            || avoid_walking_into(ctx, pb0)
            || avoid_walking_into(ctx, pb2)
            || (a_top && a_mid && can_walk_on_state(ctx, x, y, dest_z, pb0_state)) // we could just ascend
            || (b_top && b_mid && can_walk_on_state(ctx, dest_x, y, z, pb2_state))
            || (!a_top && a_mid && a_low) // head bonk A
            || (!b_top && b_mid && b_low)
        {
            return;
        }
        res.cost = multiplier * SQRT_2 + jump_one_block_cost();
        res.x = dest_x;
        res.z = dest_z;
        res.y = y + 1;
        return;
    }
    let mut option_a = get_mining_duration_ticks_state(ctx, x, y, dest_z, pb0_state, false);
    let mut option_b = get_mining_duration_ticks_state(ctx, dest_x, y, z, pb2_state, false);
    if option_a != 0.0 && option_b != 0.0 {
        // Both lower corners need breaking already: no way around.
        return;
    }
    let pb1_state = ctx.get(x, y + 1, dest_z);
    let pb1 = ctx.table.get(pb1_state);
    option_a += get_mining_duration_ticks_state(ctx, x, y + 1, dest_z, pb1_state, true);
    if option_a != 0.0 && option_b != 0.0 {
        return;
    }
    let pb3_state = ctx.get(dest_x, y + 1, z);
    let pb3 = ctx.table.get(pb3_state);
    if option_a == 0.0
        && ((avoid_walking_into(ctx, pb2) && !(pb2.is(flags::LIQUID_BLOCK) && pb2.is_water()))
            || avoid_walking_into(ctx, pb3))
    {
        // Option A is fully costed; edging around that side would walk into something bad.
        return;
    }
    option_b += get_mining_duration_ticks_state(ctx, dest_x, y + 1, z, pb3_state, true);
    if option_a != 0.0 && option_b != 0.0 {
        return;
    }
    if option_b == 0.0
        && ((avoid_walking_into(ctx, pb0) && !(pb0.is(flags::LIQUID_BLOCK) && pb0.is_water()))
            || avoid_walking_into(ctx, pb1))
    {
        return;
    }
    if option_a != 0.0 || option_b != 0.0 {
        multiplier *= SQRT_2 - 0.001; // TODO tune
        if is_climbable(start) {
            // Edging around would climb the ladder or vine instead of moving sideways.
            return;
        }
    } else if ctx.can_sprint && !water && !sneaking {
        // Not edging around and not in water: sprint.
        multiplier *= SPRINT_MULTIPLIER;
    }
    res.cost = multiplier * SQRT_2;
    if descend {
        res.cost += fall_n_blocks_cost(1).max(CENTER_AFTER_FALL_COST);
        res.y = y - 1;
    } else {
        res.y = y;
    }
    res.x = dest_x;
    res.z = dest_z;
}

pub(super) fn safe_to_cancel(m: &Movement, ex: &Exec) -> bool {
    let ctx = ex.ctx;
    let world = &ctx.world;
    let (src, dest) = (m.src, m.dest);
    let pos = ctx.player.position;
    let offset = 0.25;
    let (x, y, z) = (pos.x, pos.y - 1.0, pos.z);
    let feet = ctx.player_feet();
    if feet == src {
        return true;
    }
    // Both corners walkable.
    if can_walk_on_pos(world, BlockPos::new(src.x, src.y - 1, dest.z))
        && can_walk_on_pos(world, BlockPos::new(dest.x, src.y - 1, src.z))
    {
        return true;
    }
    // In a likely unwalkable corner: is anything holding us up?
    if feet == BlockPos::new(src.x, src.y, dest.z) || feet == BlockPos::new(dest.x, src.y, src.z) {
        return can_walk_on_pos(world, BlockPos::of(x + offset, y, z + offset))
            || can_walk_on_pos(world, BlockPos::of(x + offset, y, z - offset))
            || can_walk_on_pos(world, BlockPos::of(x - offset, y, z + offset))
            || can_walk_on_pos(world, BlockPos::of(x - offset, y, z - offset));
    }
    true
}

pub(super) fn update_state(m: &mut Movement, state: &mut MovementState, ex: &mut Exec) {
    m.base_update_state(state, ex);
    if state.status != MovementStatus::Running {
        return;
    }
    let ctx = ex.ctx;
    let world = &ctx.world;
    let (src, dest) = (m.src, m.dest);
    let feet = ctx.player_feet();
    if feet == dest {
        state.set_status(MovementStatus::Success);
        return;
    } else if !m.player_in_valid_position(ex)
        && !(is_liquid(world.info_pos(src)) && m.valid_positions().contains(&feet.above()))
    {
        state.set_status(MovementStatus::Unreachable);
        return;
    }
    if dest.y > src.y && ctx.player.position.y < src.y as f64 + 0.1 && ctx.player.horizontal_collision {
        state.set_input(Input::Jump, true);
    }
    if sprint(m, ex) {
        state.set_input(Input::Sprint, true);
    }
    let on_magma = ctx.settings.allow_walk_on_magma_blocks
        && ctx
            .stepping_on_blocks()
            .iter()
            .any(|b| world.info_pos(*b).is(flags::MAGMA));
    state.set_input(Input::Sneak, on_magma);
    move_towards(ex, state, dest);
}

fn sprint(m: &Movement, ex: &Exec) -> bool {
    let world = &ex.ctx.world;
    if is_liquid(world.info_pos(ex.ctx.player_feet())) && !ex.ctx.settings.sprint_in_water {
        return false;
    }
    m.positions_to_break[0..4]
        .iter()
        .all(|p| can_walk_through_pos(world, *p))
}
