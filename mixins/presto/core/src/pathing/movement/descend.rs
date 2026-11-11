//! `MovementDescend`: one block sideways and one down; its cost function also finds falls (`MovementFall`).

use super::{Exec, MoveResult, Movement, MovementKind, MovementState, MovementStatus, MovementTarget, move_towards};
use crate::behavior::input::Input;
use crate::math::{BlockPos, Vec3};
use crate::pathing::context::CalculationContext;
use crate::pathing::costs::*;
use crate::pathing::helper::*;
use crate::world::{StateId, flags};

pub fn cost(ctx: &CalculationContext, x: i32, y: i32, z: i32, dest_x: i32, dest_z: i32, res: &mut MoveResult) {
    let mut total = 0.0;
    let dest_down_state = ctx.get(dest_x, y - 1, dest_z);
    total += get_mining_duration_ticks_state(ctx, dest_x, y - 1, dest_z, dest_down_state, false);
    if total >= COST_INF {
        return;
    }
    total += get_mining_duration_ticks(ctx, dest_x, y, dest_z, false);
    if total >= COST_INF {
        return;
    }
    // Only the top of the three needs the falling blocks above it counted.
    total += get_mining_duration_ticks(ctx, dest_x, y + 1, dest_z, true);
    if total >= COST_INF {
        return;
    }
    let from_down = ctx.info(x, y - 1, z);
    if is_climbable(from_down) {
        return;
    }
    //  A
    // SA
    //  A
    //  B
    //  C
    //  D
    // Starting at S, B must be air for a fall; A can be broken by either; C, D... set the fall's length.
    let below = ctx.get(dest_x, y - 2, dest_z);
    if !can_walk_on_state(ctx, dest_x, y - 2, dest_z, below) {
        dynamic_fall_cost(ctx, x, y, z, dest_x, dest_z, total, below, res);
        return;
    }
    let dest_down = ctx.table.get(dest_down_state);
    if dest_down.is(flags::LADDER) || dest_down.is(flags::VINE) {
        return;
    }
    if can_use_frost_walker(ctx, dest_down) {
        return; // it will freeze when we walk into it (no need to check assumeWalkOnWater)
    }
    // Half the block plus 0.3 to the edge, then the other 0.2 while falling (in parallel, hence the max).
    let mut walk = WALK_OFF_BLOCK_COST;
    if from_down.is(flags::SOUL_SAND) {
        walk *= WALK_ONE_OVER_SOUL_SAND_COST / WALK_ONE_BLOCK_COST;
    }
    total += walk + fall_n_blocks_cost(1).max(CENTER_AFTER_FALL_COST);
    res.x = dest_x;
    res.y = y - 1;
    res.z = dest_z;
    res.cost = total;
}

/// `dynamicFallCost`: the end of a fall down the column at `dest_x, dest_z`, onto ground, into water, onto a
/// ladder, or with a water bucket. Returns whether the fall needs the bucket.
pub fn dynamic_fall_cost(
    ctx: &CalculationContext,
    _x: i32,
    y: i32,
    _z: i32,
    dest_x: i32,
    dest_z: i32,
    front_break: f64,
    below: StateId,
    res: &mut MoveResult,
) -> bool {
    if front_break != 0.0 && ctx.info(dest_x, y + 2, dest_z).is(flags::FALLING_BLOCK) {
        // Breaking in front would let it fall down this column, maybe into the water we land in.
        return false;
    }
    if !can_walk_through_state(ctx, dest_x, y - 2, dest_z, below) {
        return false;
    }
    let mut cost_so_far = 0.0;
    let mut effective_start_height = y;
    let mut fall_height = 3;
    loop {
        let new_y = y - fall_height;
        if new_y < ctx.min_y {
            // Into the void (in the End, say).
            return false;
        }
        let reached_minimum = fall_height >= ctx.min_fall_height;
        let onto_state = ctx.get(dest_x, new_y, dest_z);
        let onto = ctx.table.get(onto_state);
        let unprotected = fall_height - (y - effective_start_height);
        let tentative = WALK_OFF_BLOCK_COST
            + fall_n_blocks_cost(unprotected.max(0) as usize)
            + front_break
            + cost_so_far;
        if reached_minimum && is_water(onto) {
            if !can_walk_through_state(ctx, dest_x, new_y, dest_z, onto_state) {
                return false;
            }
            if ctx.assume_walk_on_water {
                return false;
            }
            if is_flowing(ctx, dest_x, new_y, dest_z, onto) {
                return false;
            }
            if !can_walk_on(ctx, dest_x, new_y - 1, dest_z) {
                // We could punch right through the water into something else.
                return false;
            }
            res.x = dest_x;
            res.y = new_y;
            res.z = dest_z;
            res.cost = tentative;
            return false;
        }
        if reached_minimum && ctx.allow_fall_into_lava && is_lava(onto) {
            res.x = dest_x;
            res.y = new_y;
            res.z = dest_z;
            res.cost = tentative;
            return false;
        }
        if unprotected <= 11 && is_climbable(onto) {
            // Falling 11 or more, we don't grab vines or ladders. This resets our speed.
            cost_so_far += fall_n_blocks_cost((unprotected - 1).max(0) as usize);
            cost_so_far += LADDER_DOWN_ONE_COST;
            effective_start_height = new_y;
            fall_height += 1;
            continue;
        }
        if can_walk_through_state(ctx, dest_x, new_y, dest_z, onto_state) {
            fall_height += 1;
            continue;
        }
        if !can_walk_on_state(ctx, dest_x, new_y, dest_z, onto_state) {
            return false;
        }
        if is_bottom_slab(onto) {
            return false; // falling onto a half slab is glitchy, and can hurt more than expected
        }
        if reached_minimum && unprotected <= ctx.max_fall_height_no_water + 1 {
            res.x = dest_x;
            res.y = new_y + 1;
            res.z = dest_z;
            res.cost = tentative;
            return false;
        }
        if reached_minimum && ctx.has_water_bucket && unprotected <= ctx.max_fall_height_bucket + 1 {
            res.x = dest_x;
            res.y = new_y + 1; // the block we fall onto; we end above it
            res.z = dest_z;
            res.cost = tentative + ctx.place_bucket_cost();
            return true;
        }
        return false;
    }
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
    let pos = ctx.player.position;
    let fake_dest = BlockPos::new(dest.x * 2 - src.x, dest.y, dest.z * 2 - src.z);
    if (feet == dest || feet == fake_dest) && (is_liquid(world.info_pos(dest)) || pos.y - (dest.y as f64) < 0.5) {
        // Wait to be on the ground (or in water, or on a lily pad) before saying we're done.
        state.set_status(MovementStatus::Success);
        return;
    }
    if safe_mode(m, world) {
        let dx = (src.x as f64 + 0.5) * 0.17 + (dest.x as f64 + 0.5) * 0.83;
        let dz = (src.z as f64 + 0.5) * 0.17 + (dest.z as f64 + 0.5) * 0.83;
        let rot = ctx
            .rotation_to(Vec3::new(dx, dest.y as f64, dz))
            .with_pitch(ctx.player_rotations().pitch);
        state
            .set_target(MovementTarget::new(rot, false))
            .set_input(Input::MoveForward, true);
        return;
    }
    let diff_x = pos.x - (dest.x as f64 + 0.5);
    let diff_z = pos.z - (dest.z as f64 + 0.5);
    let ab = (diff_x * diff_x + diff_z * diff_z).sqrt();
    let x = pos.x - (src.x as f64 + 0.5);
    let z = pos.z - (src.z as f64 + 0.5);
    let from_start = (x * x + z * z).sqrt();
    let on_magma = ctx.settings.allow_walk_on_magma_blocks
        && world
            .info_pos(BlockPos::of_vec(pos).below())
            .is(flags::MAGMA);
    state.set_input(Input::Sneak, on_magma);
    if feet != dest || ab > 0.25 {
        let num_ticks = if let MovementKind::Descend { num_ticks, .. } = &mut m.kind {
            let n = *num_ticks;
            *num_ticks += 1;
            n
        } else {
            0
        };
        if num_ticks < 20 && from_start < 1.25 {
            move_towards(ex, state, fake_dest);
        } else {
            move_towards(ex, state, dest);
        }
    }
}

/// `forceSafeMode`, from the path executor that knows the next movement.
pub fn force_safe_mode(m: &mut Movement) {
    if let MovementKind::Descend { force_safe_mode, .. } = &mut m.kind {
        *force_safe_mode = true;
    }
}

/// `safeMode`: walk off slowly rather than sprint through, as something past the edge is dangerous.
pub fn safe_mode(m: &Movement, world: &CalculationContext) -> bool {
    if let MovementKind::Descend {
        force_safe_mode: true,
        ..
    } = m.kind
    {
        return true;
    }
    // One more block along the descend: what we'd run into sprinting straight through it.
    let into = m.dest.subtract(m.src.below()).add(m.dest);
    if skip_to_ascend(m, world) {
        // We can overshoot and glitch up onto it.
        return true;
    }
    (0..=2).any(|y| avoid_walking_into(world, world.info_pos(into.above_n(y))))
}

/// `skipToAscend`.
pub fn skip_to_ascend(m: &Movement, world: &CalculationContext) -> bool {
    let into = m.dest.subtract(m.src.below()).add(m.dest);
    !can_walk_through_pos(world, into)
        && can_walk_through_pos(world, into.above())
        && can_walk_through_pos(world, into.above_n(2))
}
