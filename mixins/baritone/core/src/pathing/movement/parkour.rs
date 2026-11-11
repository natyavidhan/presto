//! `MovementParkour`: a sprint jump over a gap of one to three blocks, landing level or a block up, placing
//! the landing block if needed.

use super::{
    Exec, MoveResult, Movement, MovementKind, MovementState, MovementStatus, PlaceResult,
    attempt_to_place_a_block, move_towards,
};
use crate::behavior::input::Input;
use crate::math::{BlockPos, Direction};
use crate::pathing::context::CalculationContext;
use crate::pathing::costs::*;
use crate::pathing::helper::*;
use crate::world::flags;

/// `MovementParkour.cost(context, src, direction)`: the movement the search would make from `src`.
pub fn movement(ctx: &CalculationContext, src: BlockPos, direction: Direction) -> Movement {
    let mut res = MoveResult::new();
    cost(ctx, src.x, src.y, src.z, direction, &mut res);
    let dist = (res.x - src.x).abs() + (res.z - src.z).abs();
    Movement::parkour(src, dist, direction, res.y > src.y)
}

pub fn cost(ctx: &CalculationContext, x: i32, y: i32, z: i32, dir: Direction, res: &mut MoveResult) {
    if !ctx.allow_parkour {
        return;
    }
    if !ctx.allow_jump_at_build_limit && y >= ctx.max_y {
        return;
    }
    let [x_diff, _, z_diff] = dir.normal();
    if !fully_passable(ctx, x + x_diff, y, z + z_diff) {
        // Most common: the adjacent block isn't air.
        return;
    }
    let adj_state = ctx.get(x + x_diff, y - 1, z + z_diff);
    let adj = ctx.table.get(adj_state);
    if can_walk_on_state(ctx, x + x_diff, y - 1, z + z_diff, adj_state) {
        // Second most common: a traverse would do.
        return;
    }
    if avoid_walking_into(ctx, adj) && !adj.is_water() {
        return; // magma sucks
    }
    if !fully_passable(ctx, x + x_diff, y + 1, z + z_diff) {
        return;
    }
    if !fully_passable(ctx, x + x_diff, y + 2, z + z_diff) {
        return;
    }
    if !fully_passable(ctx, x, y + 2, z) {
        return;
    }
    let standing_on = ctx.info(x, y - 1, z);
    if is_climbable(standing_on) || standing_on.is(flags::STAIRS) || is_bottom_slab(standing_on) {
        return;
    }
    // With assumeWalkOnWater we can't be sure (frozen) water is frozen.
    if ctx.assume_walk_on_water && standing_on.has_fluid() {
        return;
    }
    if ctx.info(x, y, z).has_fluid() {
        return; // can't jump out of water
    }
    let max_jump = if ctx.allow_walk_on_magma_blocks && standing_on.is(flags::MAGMA) {
        2
    } else if standing_on.is(flags::SOUL_SAND) {
        2 // a one block gap
    } else if ctx.can_sprint {
        4
    } else {
        3
    };
    // From the shortest jump to the longest, check walls and landings.
    let mut verified_max_jump = 1;
    for i in 2..=max_jump {
        let dest_x = x + x_diff * i;
        let dest_z = z + z_diff * i;
        // Head and feet.
        if !fully_passable(ctx, dest_x, y + 1, dest_z) {
            break;
        }
        if !fully_passable(ctx, dest_x, y + 2, dest_z) {
            break;
        }
        // A landing a block up.
        let dest_into = ctx.bsi.get(dest_x, y, dest_z);
        if !fully_passable_state(ctx, dest_into) {
            if i <= 3
                && ctx.allow_parkour_ascend
                && ctx.can_sprint
                && can_walk_on_state(ctx, dest_x, y, dest_z, dest_into)
                && check_overshoot_safety(ctx, dest_x + x_diff, y + 1, dest_z + z_diff)
            {
                res.x = dest_x;
                res.y = y + 1;
                res.z = dest_z;
                res.cost = i as f64 * SPRINT_ONE_BLOCK_COST + ctx.jump_penalty;
                return;
            }
            break;
        }
        // A level landing. Farmland must be walkable or farming couldn't work, but never land a jump on it.
        // Frost Walker works here because we can't jump from possibly unfrozen water.
        let landing_state = ctx.bsi.get(dest_x, y - 1, dest_z);
        let landing = ctx.table.get(landing_state);
        if (!landing.is(flags::FARMLAND) && can_walk_on_state(ctx, dest_x, y - 1, dest_z, landing_state))
            || ((ctx.frost_walker + 2).min(16) >= i && can_use_frost_walker(ctx, landing))
        {
            if check_overshoot_safety(ctx, dest_x + x_diff, y, dest_z + z_diff) {
                res.x = dest_x;
                res.y = y;
                res.z = dest_z;
                res.cost = cost_from_jump_distance(i) + ctx.jump_penalty;
                return;
            }
            break;
        }
        if !fully_passable(ctx, dest_x, y + 3, dest_z) {
            break;
        }
        verified_max_jump = i;
    }
    // Parkour place: from the longest jump to the shortest, look for a landing to place.
    if !ctx.allow_parkour_place {
        return;
    }
    let mut i = verified_max_jump;
    while i > 1 {
        let dest_x = x + i * x_diff;
        let dest_z = z + i * z_diff;
        let to_replace = ctx.info(dest_x, y - 1, dest_z);
        let place_cost = ctx.cost_of_placing_at(dest_x, y - 1, dest_z, to_replace);
        if place_cost >= COST_INF {
            i -= 1;
            continue;
        }
        if !is_replaceable(ctx, dest_x, y - 1, dest_z, to_replace) {
            i -= 1;
            continue;
        }
        if !check_overshoot_safety(ctx, dest_x + x_diff, y, dest_z + z_diff) {
            i -= 1;
            continue;
        }
        for d in Direction::HORIZONTALS_AND_DOWN {
            let [dx, dy, dz] = d.normal();
            let (ax, ay, az) = (dest_x + dx, y - 1 + dy, dest_z + dz);
            if ax == dest_x - x_diff && az == dest_z - z_diff {
                continue; // we can't turn around that fast
            }
            if can_place_against(ctx, ax, ay, az) {
                res.x = dest_x;
                res.y = y;
                res.z = dest_z;
                res.cost = cost_from_jump_distance(i) + place_cost + ctx.jump_penalty;
                return;
            }
        }
        i -= 1;
    }
}

/// We walk into the two blocks past the landing anyway: they mustn't be anything to avoid.
fn check_overshoot_safety(ctx: &CalculationContext, x: i32, y: i32, z: i32) -> bool {
    !avoid_walking_into(ctx, ctx.info(x, y, z)) && !avoid_walking_into(ctx, ctx.info(x, y + 1, z))
}

fn cost_from_jump_distance(dist: i32) -> f64 {
    match dist {
        2 => WALK_ONE_BLOCK_COST * 2.0, // IDK LOL
        3 => WALK_ONE_BLOCK_COST * 3.0,
        4 => SPRINT_ONE_BLOCK_COST * 4.0,
        _ => panic!("LOL {dist}"),
    }
}

pub(super) fn update_state(m: &mut Movement, state: &mut MovementState, ex: &mut Exec) {
    m.base_update_state(state, ex);
    if state.status != MovementStatus::Running {
        return;
    }
    let ctx = ex.ctx;
    let world = &ctx.world;
    let MovementKind::Parkour {
        direction,
        dist,
        ascend,
    } = m.kind
    else {
        return;
    };
    let (src, dest) = (m.src, m.dest);
    let feet = ctx.player_feet();
    let pos = ctx.player.position;
    if feet.y < src.y {
        // We have fallen.
        ex.log_debug("sorry");
        state.set_status(MovementStatus::Unreachable);
        return;
    }
    if dist >= 4 || ascend {
        state.set_input(Input::Sprint, true);
    }
    if ctx.settings.allow_walk_on_magma_blocks && world.info_pos(feet.below()).is(flags::MAGMA) {
        state.set_input(Input::Sneak, true);
    }
    move_towards(ex, state, dest);
    if feet == dest {
        let d = world.info_pos(dest);
        if d.is(flags::VINE) || d.is(flags::LADDER) {
            // It physically hurt to support parkour jumping onto a vine, but it's done.
            state.set_status(MovementStatus::Success);
            return;
        }
        if pos.y - (feet.y as f64) < 0.094 {
            // 0.094 for lily pads.
            state.set_status(MovementStatus::Success);
        }
    } else if feet != src {
        if feet == src.relative(direction) || pos.y - src.y as f64 > 0.0001 {
            if ctx.settings.allow_place // see PR #3775
                && ex.has_generic_throwaway()
                && !can_walk_on_pos(world, dest.below())
                && !ctx.player.on_ground
                && attempt_to_place_a_block(state, ex, dest.below(), true, false) == PlaceResult::ReadyToPlace
            {
                // Prefer down first: looking sideways mid-air could spoil the jump.
                state.set_input(Input::ClickRight, true);
            }
            // Don't jump too late over a two block gap.
            if dist == 3 && !ascend {
                let x_diff = (src.x as f64 + 0.5) - pos.x;
                let z_diff = (src.z as f64 + 0.5) - pos.z;
                let dist_from_start = x_diff.abs().max(z_diff.abs());
                if dist_from_start < 0.7 {
                    return;
                }
            }
            state.set_input(Input::Jump, true);
        } else if feet != dest.relative_n(direction, -1) {
            state.set_input(Input::Sprint, false);
            if feet == src.relative_n(direction, -1) {
                move_towards(ex, state, src);
            } else {
                move_towards(ex, state, src.relative_n(direction, -1));
            }
        }
    }
}
