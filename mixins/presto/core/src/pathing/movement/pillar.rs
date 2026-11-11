//! `MovementPillar`: straight up, placing a block under ourselves (or climbing a ladder or vine, or swimming up
//! a water column).

use super::{Exec, Movement, MovementState, MovementStatus, MovementTarget};
use crate::behavior::input::Input;
use crate::pathing::context::CalculationContext;
use crate::pathing::costs::*;
use crate::pathing::helper::*;
use crate::world::{SlabType, flags};

pub fn cost(ctx: &CalculationContext, x: i32, y: i32, z: i32) -> f64 {
    let from_state = ctx.get(x, y, z);
    let from = ctx.table.get(from_state);
    let ladder = is_climbable(from);
    let from_down_state = ctx.get(x, y - 1, z);
    let from_down = ctx.table.get(from_down_state);
    if !ladder {
        if is_climbable(from_down) {
            return COST_INF; // can't pillar from a ladder or vine onto something that isn't climbable
        }
        if from_down.is(flags::SLAB) && from_down.slab == SlabType::Bottom {
            return COST_INF; // can't pillar up from a bottom slab onto a non-ladder
        }
    }
    let to_break_state = ctx.get(x, y + 2, z);
    let to_break = ctx.table.get(to_break_state);
    if to_break.is(flags::FENCE_GATE) {
        return COST_INF; // issue #172
    }
    let mut src_up = None;
    if is_water(to_break) && is_water(from) {
        let up = ctx.info(x, y + 1, z);
        src_up = Some(up);
        if is_water(up) {
            return LADDER_UP_ONE_COST; // up a water column we're already in
        }
    }
    let mut place_cost = 0.0;
    if !ladder {
        // A block goes where we started, to jump onto.
        place_cost = ctx.cost_of_placing_at(x, y, z, from);
        if place_cost >= COST_INF {
            return COST_INF;
        }
        if from_down.is_air() {
            place_cost += 0.1; // a hair worse to pillar on what is currently air
        }
    }
    if (is_liquid(from) && !can_place_against_info(ctx, x, z, from_down))
        || (is_liquid(from_down) && ctx.assume_walk_on_water)
    {
        // Standing in water we can't pillar; standing on water with jesus neither. (On water without jesus we
        // ascended or sneak-backplaced to get here, so it is possible.)
        return COST_INF;
    }
    if (from.is(flags::LILY_PAD) || from.is(flags::CARPET)) && from_down.has_fluid() {
        // We'd have to break what we stand on.
        return COST_INF;
    }
    let mut hardness = get_mining_duration_ticks_state(ctx, x, y + 2, z, to_break_state, true);
    if hardness >= COST_INF {
        return COST_INF;
    }
    if hardness != 0.0 {
        if is_climbable(to_break) {
            hardness = 0.0; // we'll use the ladder or vine, not break it
        } else {
            let check = ctx.info(x, y + 3, z); // could the block above it fall on us?
            if check.is(flags::FALLING_BLOCK) {
                let up = src_up.unwrap_or_else(|| ctx.info(x, y + 1, z));
                if !to_break.is(flags::FALLING_BLOCK) || !up.is(flags::FALLING_BLOCK) {
                    return COST_INF;
                }
            }
        }
    }
    if ladder {
        LADDER_UP_ONE_COST + hardness * 5.0
    } else {
        jump_one_block_cost() + place_cost + ctx.jump_penalty + hardness
    }
}

pub(super) fn prepared(m: &mut Movement, state: &mut MovementState, ex: &mut Exec) -> bool {
    let world = &ex.ctx.world;
    let feet = ex.ctx.player_feet();
    if (feet == m.src || feet == m.src.below()) && is_climbable(world.info_pos(m.src.below())) {
        state.set_input(Input::Sneak, true);
    }
    if is_water(world.info_pos(m.dest.above())) {
        return true;
    }
    m.base_prepared(state, ex)
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
    if feet.y < src.y {
        state.set_status(MovementStatus::Unreachable);
        return;
    }
    let from_down = world.info_pos(src);
    if is_water(from_down) && is_water(world.info_pos(dest)) {
        // Stay centred while swimming up a water column.
        state.set_target(MovementTarget::new(ctx.rotation_to(dest.center()), false));
        let c = dest.center();
        if (pos.x - c.x).abs() > 0.2 || (pos.z - c.z).abs() > 0.2 {
            state.set_input(Input::MoveForward, true);
        }
        if feet == dest {
            state.set_status(MovementStatus::Success);
        }
        return;
    }
    let ladder = is_climbable(from_down);
    let place = m.position_to_place.expect("pillar places where it starts");
    let rotation = ctx.rotation_to(place.center());
    if !ladder {
        state.set_target(MovementTarget::new(
            ctx.player_rotations().with_pitch(rotation.pitch),
            true,
        ));
    }
    let mut block_is_there = can_walk_on_pos(world, src) || ladder;
    if ladder {
        if feet == dest {
            state.set_status(MovementStatus::Success);
            return;
        }
        super::move_towards(ex, state, dest);
        state.set_input(Input::Jump, true);
        return;
    }
    // Get ready to place a throwaway block.
    if !ex.select_throwaway_for_location(true, src.x, src.y, src.z) {
        state.set_status(MovementStatus::Unreachable);
        return;
    }
    // Sneak now: we only right click once crouched, the tick after.
    state.set_input(Input::Sneak, true);
    let diff_x = pos.x - (dest.x as f64 + 0.5);
    let diff_z = pos.z - (dest.z as f64 + 0.5);
    let dist = (diff_x * diff_x + diff_z * diff_z).sqrt();
    let v = ctx.player.velocity;
    let flat_motion = (v.x * v.x + v.z * v.z).sqrt();
    if dist > 0.17 {
        // Less than 0.2, because of the 0.3 sneak edge limit: move toward the middle, facing where we place.
        state.set_input(Input::MoveForward, true);
        state.set_target(MovementTarget::new(rotation, true));
    } else if flat_motion < 0.05 {
        // Above the goal height, stop jumping.
        state.set_input(Input::Jump, pos.y < dest.y as f64);
    }
    if !block_is_there {
        let fr = world.info_pos(src);
        if !(fr.is_air() || fr.is(flags::REPLACEABLE)) {
            if let Some(rot) = ex.reachable_with(src, ctx.block_reach_distance()) {
                state.set_target(MovementTarget::new(rot, true));
            }
            state.set_input(Input::Jump, false); // breaking is about 5 times slower while jumping
            state.set_input(Input::ClickLeft, true);
            block_is_there = false;
        } else if ex.crouching()
            && (ex.will_look_at(src.below(), state.target.rotation) || ex.will_look_at(src, state.target.rotation))
            && pos.y > dest.y as f64 + 0.1
        {
            state.set_input(Input::ClickRight, true);
        }
    }
    // At the goal with the block placed under us.
    if feet == dest && block_is_there {
        state.set_status(MovementStatus::Success);
    }
}
