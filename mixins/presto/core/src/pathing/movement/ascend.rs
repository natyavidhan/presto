//! `MovementAscend`: one block sideways and one up, jumping (placing the step if needed).

use super::{
    Exec, Movement, MovementKind, MovementState, MovementStatus, PlaceResult, attempt_to_place_a_block,
    move_towards,
};
use crate::behavior::input::Input;
use crate::math::Direction;
use crate::pathing::context::CalculationContext;
use crate::pathing::costs::*;
use crate::pathing::helper::*;
use crate::world::flags;

pub fn cost(ctx: &CalculationContext, x: i32, y: i32, z: i32, dest_x: i32, dest_z: i32) -> f64 {
    let to_place_state = ctx.get(dest_x, y, dest_z);
    let to_place = ctx.table.get(to_place_state);
    let mut additional_placement_cost = 0.0;
    if !can_walk_on_state(ctx, dest_x, y, dest_z, to_place_state) {
        additional_placement_cost = ctx.cost_of_placing_at(dest_x, y, dest_z, to_place);
        if additional_placement_cost >= COST_INF {
            return COST_INF;
        }
        if !is_replaceable(ctx, dest_x, y, dest_z, to_place) {
            return COST_INF;
        }
        let mut found_place_option = false;
        for dir in Direction::HORIZONTALS_AND_DOWN {
            let [dx, dy, dz] = dir.normal();
            let (ax, ay, az) = (dest_x + dx, y + dy, dest_z + dz);
            if ax == x && az == z {
                // We might backplace now, but it'll be broken by the time we'd need it.
                continue;
            }
            if can_place_against(ctx, ax, ay, az) {
                found_place_option = true;
                break;
            }
        }
        if !found_place_option {
            return COST_INF;
        }
    }
    let src_up2_state = ctx.get(x, y + 2, z);
    let src_up2 = ctx.table.get(src_up2_state);
    if ctx.info(x, y + 3, z).is(flags::FALLING_BLOCK)
        && (can_walk_through(ctx, x, y + 1, z) || !src_up2.is(flags::FALLING_BLOCK))
    {
        // It would fall on us and possibly suffocate us. (Standing at the start presupposes that whole stack
        // above our head was already cleared, so only a block that would newly fall counts.)
        return COST_INF;
    }
    let src_down = ctx.info(x, y - 1, z);
    if is_climbable(src_down) {
        return COST_INF;
    }
    // We can jump from soul sand, but not from a bottom slab.
    let jumping_from_bottom_slab = is_bottom_slab(src_down);
    let jumping_to_bottom_slab = is_bottom_slab(to_place);
    if jumping_from_bottom_slab && !jumping_to_bottom_slab {
        return COST_INF; // from a bottom slab, only onto another bottom slab
    }
    let mut walk;
    if jumping_to_bottom_slab {
        if jumping_from_bottom_slab {
            walk = jump_one_block_cost().max(WALK_ONE_BLOCK_COST); // space right away
            walk += ctx.jump_penalty;
        } else {
            walk = WALK_ONE_BLOCK_COST; // no jump, we walk into the slab
        }
    } else {
        if to_place.is(flags::SOUL_SAND) {
            walk = WALK_ONE_OVER_SOUL_SAND_COST;
        } else if to_place.is(flags::MAGMA) {
            walk = SNEAK_ONE_BLOCK_COST;
        } else {
            walk = jump_one_block_cost().max(WALK_ONE_BLOCK_COST);
        }
        walk += ctx.jump_penalty;
    }
    let mut total = walk + additional_placement_cost;
    // The falling check above makes include_falling unnecessary here.
    total += get_mining_duration_ticks_state(ctx, x, y + 2, z, src_up2_state, false);
    if total >= COST_INF {
        return COST_INF;
    }
    total += get_mining_duration_ticks(ctx, dest_x, y + 1, dest_z, false);
    if total >= COST_INF {
        return COST_INF;
    }
    total += get_mining_duration_ticks(ctx, dest_x, y + 2, dest_z, true);
    total
}

pub(super) fn update_state(m: &mut Movement, state: &mut MovementState, ex: &mut Exec) {
    let ctx = ex.ctx;
    let world = &ctx.world;
    if ctx.player_feet().y < m.src.y {
        // Runs even while preparing.
        state.set_status(MovementStatus::Unreachable);
        return;
    }
    m.base_update_state(state, ex);
    if state.status != MovementStatus::Running {
        return;
    }
    let (src, dest) = (m.src, m.dest);
    let feet = ctx.player_feet();
    let dir = m.direction();
    if feet == dest || feet == dest.add(dir.below()) {
        state.set_status(MovementStatus::Success);
        return;
    }
    let place = m.position_to_place.expect("ascend places under its destination");
    let jumping_onto = world.info_pos(place);
    if !can_walk_on_pos(world, place) {
        let ticks = if let MovementKind::Ascend {
            ticks_without_placement,
        } = &mut m.kind
        {
            *ticks_without_placement += 1;
            *ticks_without_placement
        } else {
            0
        };
        if attempt_to_place_a_block(state, ex, dest.below(), false, true) == PlaceResult::ReadyToPlace {
            state.set_input(Input::Sneak, true);
            if ex.crouching() {
                state.set_input(Input::ClickRight, true);
            }
        }
        if ticks > 10 {
            // Ten ticks without placing: we might be standing in the way, so back up.
            state.set_input(Input::MoveBack, true);
        }
        return;
    }
    move_towards(ex, state, dest);
    state.set_input(
        Input::Sneak,
        ctx.settings.allow_walk_on_magma_blocks && jumping_onto.is(flags::MAGMA),
    );
    if is_bottom_slab(jumping_onto) && !is_bottom_slab(world.info_pos(src.below())) {
        return; // walking from a non-slab into a bottom slab needs no jump
    }
    if ctx.settings.assume_step || feet == src.above() {
        return; // already jumping
    }
    let x_axis = (src.x - dest.x).abs();
    let z_axis = (src.z - dest.z).abs();
    let pos = ctx.player.position;
    let flat_dist_to_next = x_axis as f64 * ((dest.x as f64 + 0.5) - pos.x).abs()
        + z_axis as f64 * ((dest.z as f64 + 0.5) - pos.z).abs();
    let side_dist = z_axis as f64 * ((dest.x as f64 + 0.5) - pos.x).abs()
        + x_axis as f64 * ((dest.z as f64 + 0.5) - pos.z).abs();
    let v = ctx.player.velocity;
    let lateral_motion = x_axis as f64 * v.z + z_axis as f64 * v.x;
    if lateral_motion.abs() > 0.1 {
        return;
    }
    if head_bonk_clear(m, ex) {
        state.set_input(Input::Jump, true);
        return;
    }
    if flat_dist_to_next > 1.2 || side_dist > 0.2 {
        return;
    }
    // Pointing the right way and close enough: jump. (Earlier, we might jump before moving and fall back, or
    // bonk our head on a block beside us.)
    state.set_input(Input::Jump, true);
}

/// `headBonkClear`: nothing above the start's head on any side.
fn head_bonk_clear(m: &Movement, ex: &Exec) -> bool {
    let start_up = m.src.above_n(2);
    Direction::HORIZONTAL
        .into_iter()
        .all(|d| can_walk_through_pos(&ex.ctx.world, start_up.relative(d)))
}
