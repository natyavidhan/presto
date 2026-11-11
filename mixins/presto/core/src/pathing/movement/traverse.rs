//! `MovementTraverse`: one block sideways on the same level, bridging (placing the floor) if needed.

use super::{
    Exec, Movement, MovementKind, MovementState, MovementStatus, MovementTarget, PlaceResult,
    attempt_to_place_a_block, move_towards, move_towards_with_slight_rotation,
};
use crate::behavior::input::Input;
use crate::math::{Direction, Rotation, Vec3, rotation_utils};
use crate::pathing::context::CalculationContext;
use crate::pathing::costs::*;
use crate::pathing::helper::*;
use crate::world::{SlabType, flags};

pub fn cost(ctx: &CalculationContext, x: i32, y: i32, z: i32, dest_x: i32, dest_z: i32) -> f64 {
    let pb0 = ctx.get(dest_x, y + 1, dest_z);
    let pb1 = ctx.get(dest_x, y, dest_z);
    let dest_on_state = ctx.get(dest_x, y - 1, dest_z);
    let dest_on = ctx.table.get(dest_on_state);
    let src_down = ctx.info(x, y - 1, z);
    let standing_on_a_block = must_be_solid_to_walk_on(ctx, x, y - 1, z, src_down);
    let frost_walker = standing_on_a_block && !ctx.assume_walk_on_water && can_use_frost_walker(ctx, dest_on);
    let pb0i = ctx.table.get(pb0);
    let pb1i = ctx.table.get(pb1);
    if frost_walker || can_walk_on_state(ctx, dest_x, y - 1, dest_z, dest_on_state) {
        // A walk, not a bridge.
        let mut wc = WALK_ONE_BLOCK_COST;
        let mut water = false;
        let mut sneaking = false;
        if is_water(pb0i) || is_water(pb1i) {
            wc = ctx.water_walk_speed;
            water = true;
        } else {
            if dest_on.is(flags::SOUL_SAND) {
                wc += (WALK_ONE_OVER_SOUL_SAND_COST - WALK_ONE_BLOCK_COST) / 2.0;
            } else if frost_walker {
                // Frost Walker walks on water without the penalty, when we know we won't be using jesus.
            } else if dest_on.is(flags::LIQUID_BLOCK) && dest_on.is_water() {
                wc += ctx.walk_on_water_one_penalty;
            }
            if src_down.is(flags::SOUL_SAND) {
                wc += (WALK_ONE_OVER_SOUL_SAND_COST - WALK_ONE_BLOCK_COST) / 2.0;
            } else if ctx.allow_walk_on_magma_blocks && src_down.is(flags::MAGMA) {
                sneaking = true;
                wc += (SNEAK_ONE_BLOCK_COST - WALK_ONE_BLOCK_COST) / 2.0;
            }
        }
        let mut hardness1 = get_mining_duration_ticks_state(ctx, dest_x, y, dest_z, pb1, false);
        if hardness1 >= COST_INF {
            return COST_INF;
        }
        // Only the upper block needs the falling blocks above it counted.
        let mut hardness2 = get_mining_duration_ticks_state(ctx, dest_x, y + 1, dest_z, pb0, true);
        if hardness1 == 0.0 && hardness2 == 0.0 {
            if !water && !sneaking && ctx.can_sprint {
                // Nothing in the way, not in water, not sneak placing: sprint (soul sand doesn't stop it).
                wc *= SPRINT_MULTIPLIER;
            }
            return wc;
        }
        if is_climbable(src_down) {
            hardness1 *= 5.0;
            hardness2 *= 5.0;
        }
        wc + hardness1 + hardness2
    } else {
        // A bridge: a block has to be placed first.
        if is_climbable(src_down) {
            return COST_INF;
        }
        if !is_replaceable(ctx, dest_x, y - 1, dest_z, dest_on) {
            return COST_INF;
        }
        let through_water = is_water(pb0i) || is_water(pb1i);
        if is_water(dest_on) && through_water {
            // With assumeWalkOnWater, a traverse in water: not allowed.
            return COST_INF;
        }
        let place_cost = ctx.cost_of_placing_at(dest_x, y - 1, dest_z, dest_on);
        if place_cost >= COST_INF {
            return COST_INF;
        }
        let hardness1 = get_mining_duration_ticks_state(ctx, dest_x, y, dest_z, pb1, false);
        if hardness1 >= COST_INF {
            return COST_INF;
        }
        let hardness2 = get_mining_duration_ticks_state(ctx, dest_x, y + 1, dest_z, pb0, true);
        let mut wc = if through_water {
            ctx.water_walk_speed
        } else {
            WALK_ONE_BLOCK_COST
        };
        for dir in Direction::HORIZONTALS_AND_DOWN {
            let [dx, dy, dz] = dir.normal();
            let (ax, ay, az) = (dest_x + dx, y - 1 + dy, dest_z + dz);
            if ax == x && az == z {
                continue; // that would be a backplace
            }
            if can_place_against(ctx, ax, ay, az) {
                return wc + place_cost + hardness1 + hardness2;
            }
        }
        // No side to place against: it has to be a backplace.
        if src_down.is(flags::SOUL_SAND) || (src_down.is(flags::SLAB) && src_down.slab != SlabType::Double) {
            return COST_INF; // can't sneak and backplace against soul sand or half slabs
        }
        if !standing_on_a_block {
            return COST_INF; // standing on water / swimming
        }
        let block_src = ctx.info(x, y, z);
        if (block_src.is(flags::LILY_PAD) || block_src.is(flags::CARPET)) && src_down.has_fluid() {
            return COST_INF; // we can stand on these but can't place against them
        }
        wc *= SNEAK_ONE_BLOCK_COST / WALK_ONE_BLOCK_COST; // sneak backplacing means sneaking
        wc + place_cost + hardness1 + hardness2
    }
}

pub(super) fn prepared(m: &mut Movement, state: &mut MovementState, ex: &mut Exec) -> bool {
    let feet = ex.ctx.player_feet();
    if (feet == m.src || feet == m.src.below()) && is_climbable(ex.ctx.world.info_pos(m.src.below())) {
        state.set_input(Input::Sneak, true);
    }
    m.base_prepared(state, ex)
}

pub(super) fn update_state(m: &mut Movement, state: &mut MovementState, ex: &mut Exec) {
    m.base_update_state(state, ex);
    let ctx = ex.ctx;
    let world = &ctx.world;
    let pb0 = world.info_pos(m.positions_to_break[0]).clone();
    let pb1 = world.info_pos(m.positions_to_break[1]).clone();
    let pos = ex.ctx.player.position;
    let (src, dest) = (m.src, m.dest);
    if state.status != MovementStatus::Running {
        // walkWhileBreaking: approach while mining the block in front, when that is safe.
        if !ex.ctx.settings.walk_while_breaking {
            return;
        }
        if state.status != MovementStatus::Prepping {
            return;
        }
        if avoid_walking_into(world, &pb0) || avoid_walking_into(world, &pb1) {
            return;
        }
        let dist = (pos.x - (dest.x as f64 + 0.5))
            .abs()
            .max((pos.z - (dest.z as f64 + 0.5)).abs());
        if dist < 0.83 {
            return; // already up against the block
        }
        let Some(target) = state.target.rotation else {
            // Rare: the server sent the falling sand entity late.
            return;
        };
        // The yaw toward the middle of the destination, the pitch toward the block being broken: the two
        // blocks of a traverse are stacked, so they share a yaw.
        let yaw_to_dest = ex.ctx.rotation_to(ex.ctx.calculate_block_center(dest)).yaw;
        let mut pitch_to_break = target.pitch;
        if is_block_normal_cube(&pb0) || (pb0.is_air() && (is_block_normal_cube(&pb1) || pb1.is_air())) {
            // Before we're right up against it, this angle breaks efficiently.
            pitch_to_break = 26.0;
        }
        state
            .set_target(MovementTarget::new(Rotation::new(yaw_to_dest, pitch_to_break), true))
            .set_input(Input::MoveForward, true)
            .set_input(Input::Sprint, true);
        return;
    }

    let fd = world.info_pos(src.below());
    let ladder = is_climbable(fd);

    // Sneak may have been set while mining an adjacent block; keep it if the player is about to step on magma.
    let on_magma = ex.ctx.settings.allow_walk_on_magma_blocks
        && ex
            .ctx
            .stepping_on_blocks()
            .iter()
            .any(|b| world.info_pos(*b).is(flags::MAGMA));
    state.set_input(Input::Sneak, on_magma);

    if pb0.is(flags::DOOR) || pb1.is(flags::DOOR) {
        let not_passable = (pb0.is(flags::DOOR) && !is_door_passable(world, src, dest))
            || (pb1.is(flags::DOOR) && !is_door_passable(world, dest, src));
        let can_open = !(pb0.is(flags::IRON_DOOR) || pb1.is(flags::IRON_DOOR));
        if not_passable && can_open {
            let rot = ex.ctx.rotation_to(ex.ctx.calculate_block_center(m.positions_to_break[0]));
            state
                .set_target(MovementTarget::new(rot, true))
                .set_input(Input::ClickRight, true);
            return;
        }
    }

    if pb0.is(flags::FENCE_GATE) || pb1.is(flags::FENCE_GATE) {
        let blocked = if !is_gate_passable(world, m.positions_to_break[0], src.above()) {
            Some(m.positions_to_break[0])
        } else if !is_gate_passable(world, m.positions_to_break[1], src) {
            Some(m.positions_to_break[1])
        } else {
            None
        };
        if let Some(blocked) = blocked
            && let Some(rot) = ex.reachable(blocked, false)
        {
            state
                .set_target(MovementTarget::new(rot, true))
                .set_input(Input::ClickRight, true);
            return;
        }
    }

    let place = m.position_to_place.expect("traverse places below its destination");
    let is_the_bridge_block_there = can_walk_on_pos(world, place)
        || ladder
        || can_use_frost_walker(world, world.info_pos(place));
    let feet = ex.ctx.player_feet();
    if feet.y != dest.y && !ladder {
        ex.log_debug("Wrong Y coordinate");
        if feet.y < dest.y {
            state.set_input(Input::Jump, true);
        }
        return;
    }

    if is_the_bridge_block_there {
        if feet == dest {
            state.set_status(MovementStatus::Success);
            return;
        }
        let dir = m.direction();
        if ex.ctx.settings.overshoot_traverse && (feet == dest.add(dir) || feet == dest.add(dir).add(dir)) {
            state.set_status(MovementStatus::Success);
            return;
        }
        let low = world.info_pos(src);
        let high = world.info_pos(src.above());
        if pos.y > src.y as f64 + 0.1 && !ex.ctx.player.on_ground && (is_climbable(low) || is_climbable(high)) {
            // Pressing forward could climb the ladder instead of going forward: wait for the ground.
            return;
        }
        let into = dest.subtract(src).add(dest);
        let into_below = world.info_pos(into);
        let into_above = world.info_pos(into.above());
        let was_always_there = matches!(
            m.kind,
            MovementKind::Traverse {
                was_the_bridge_block_always_there: true
            }
        );
        if was_always_there
            && (!is_liquid(world.info_pos(feet)) || ex.ctx.settings.sprint_in_water)
            && (!avoid_walking_into(world, into_below) || is_water(into_below))
            && !avoid_walking_into(world, into_above)
        {
            state.set_input(Input::Sprint, true);
        }
        let dest_down = world.info_pos(dest.below());
        if feet.y != dest.y && ladder && is_climbable(dest_down) {
            state.set_input(Input::Jump, true);
        }
        move_towards(ex, state, m.positions_to_break[0]);
        return;
    }

    if let MovementKind::Traverse {
        was_the_bridge_block_always_there,
    } = &mut m.kind
    {
        *was_the_bridge_block_always_there = false;
    }
    let standing_on = world.info_pos(feet.below());
    if standing_on.is(flags::SOUL_SAND) || standing_on.is(flags::SLAB) {
        // Issue #118: back off the edge before placing.
        let dist = (dest.x as f64 + 0.5 - pos.x)
            .abs()
            .max((dest.z as f64 + 0.5 - pos.z).abs());
        if dist < 0.85 {
            move_towards(ex, state, dest);
            state
                .set_input(Input::MoveForward, false)
                .set_input(Input::MoveBack, true);
            return;
        }
    }
    let dist1 = (pos.x - (dest.x as f64 + 0.5))
        .abs()
        .max((pos.z - (dest.z as f64 + 0.5)).abs());
    let safe_walk = ex.ctx.settings.assume_safe_walk;
    let p = attempt_to_place_a_block(state, ex, dest.below(), false, !safe_walk);
    if (p == PlaceResult::ReadyToPlace || dist1 < 0.6) && !safe_walk {
        state.set_input(Input::Sneak, true);
    }
    match p {
        PlaceResult::ReadyToPlace => {
            if ex.crouching() || safe_walk {
                state.set_input(Input::ClickRight, true);
            }
            return;
        }
        PlaceResult::Attempting => {
            if dist1 > 0.83 {
                // Maybe go forward a bit, but only if the place we try is straight ahead.
                let yaw = ex.ctx.rotation_to(dest.center()).yaw;
                if let Some(t) = state.target.rotation
                    && (t.yaw - yaw).abs() < 0.1
                {
                    state.set_input(Input::MoveForward, true);
                }
            } else if let Some(t) = state.target.rotation
                && ex.ctx.player_rotations().is_really_close_to(t)
            {
                // Something must be in the way.
                state.set_input(Input::ClickLeft, true);
            }
            return;
        }
        PlaceResult::NoOption => {}
    }
    if feet == dest {
        // Sneaking over air in the destination: place under us, against the block we just walked off.
        let face_x = (dest.x + src.x) as f64 * 0.5 + 0.5;
        let face_y = (dest.y + src.y) as f64 * 0.5 - 0.5;
        let face_z = (dest.z + src.z) as f64 * 0.5 + 0.5;
        let goal_look = src.below();
        let back_to_face = ex.ctx.rotation_to(Vec3::new(face_x, face_y, face_z));
        let pitch = back_to_face.pitch;
        let dist2 = (pos.x - face_x).abs().max((pos.z - face_z).abs());
        if dist2 < 0.29 {
            // Issue #208: too close to the face to look at it; back up.
            let yaw = rotation_utils::calc_rotation_from_vec3d_rel(
                dest.center(),
                ex.ctx.player_head(),
                ex.ctx.player_rotations(),
            )
            .yaw;
            state.set_target(MovementTarget::new(Rotation::new(yaw, pitch), true));
            state.set_input(Input::MoveBack, true);
        } else {
            state.set_target(MovementTarget::new(back_to_face, true));
        }
        if ex.will_look_at(goal_look, state.target.rotation) {
            state.set_input(Input::ClickRight, true); // right click once we can place
            return;
        }
        if let Some(t) = state.target.rotation
            && ex.ctx.player_rotations().is_really_close_to(t)
        {
            state.set_input(Input::ClickLeft, true);
        }
        return;
    }
    move_towards_with_slight_rotation(ex, state, dest);
}
