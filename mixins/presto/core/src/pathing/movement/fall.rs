//! `MovementFall`: off an edge and down several blocks, onto ground, into water, onto a ladder, or with a water
//! bucket placed (and picked back up) at the bottom.

use super::{Exec, MoveResult, Movement, MovementState, MovementStatus, MovementTarget};
use crate::behavior::input::Input;
use crate::math::{BlockPos, Direction, Rotation, Vec3};
use crate::pathing::helper::*;
use crate::world::{Fluid, flags};

/// `buildPositionsToBreak`: the column from head height at the destination down to the landing.
pub fn build_positions_to_break(src: BlockPos, dest: BlockPos) -> Vec<BlockPos> {
    let diff_x = src.x - dest.x;
    let diff_z = src.z - dest.z;
    let diff_y = (src.y - dest.y).abs();
    (0..(diff_y + 2))
        .map(|i| BlockPos::new(src.x - diff_x, src.y + 1 - i, src.z - diff_z))
        .collect()
}

pub(super) fn prepared(m: &mut Movement, state: &mut MovementState, ex: &mut Exec) -> bool {
    if state.status == MovementStatus::Waiting {
        return true;
    }
    // Only break if one of the first three (or four) needs it; the last one might be water.
    let world = &ex.ctx.world;
    for p in m.positions_to_break.iter().take(4) {
        if !can_walk_through_pos(world, *p) {
            return m.base_prepared(state, ex);
        }
    }
    true
}

/// `willPlaceBucket`: whether the fall only works with a water bucket.
fn will_place_bucket(m: &Movement, ex: &Exec) -> bool {
    let world = &ex.ctx.world;
    let mut res = MoveResult::new();
    let below = world.get(m.dest.x, m.src.y - 2, m.dest.z);
    super::descend::dynamic_fall_cost(
        world, m.src.x, m.src.y, m.src.z, m.dest.x, m.dest.z, 0.0, below, &mut res,
    )
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
    let v = ctx.player.velocity;
    let to_dest = ctx.rotation_to(dest.center());
    let mut target_rotation: Option<Rotation> = None;
    let dest_state = world.info_pos(dest);
    if world.info_pos(dest.below()).is(flags::MAGMA)
        && ctx
            .stepping_on_blocks()
            .iter()
            .all(|b| can_walk_through_pos(world, *b))
    {
        state.set_input(Input::Sneak, true);
    }
    let is_water = dest_state.fluid == Fluid::Water;
    if !is_water && will_place_bucket(m, ex) && feet != dest {
        let bucket = ctx.inventory().find_in_hotbar("minecraft:water_bucket");
        let Some(bucket) = bucket.filter(|_| !world.bsi.dimension.is_nether()) else {
            state.set_status(MovementStatus::Unreachable);
            return;
        };
        if pos.y - (dest.y as f64) < ctx.block_reach_distance() && !ctx.player.on_ground {
            ex.inventory.select(bucket);
            target_rotation = Some(Rotation::new(to_dest.yaw, 90.0));
            if ex.will_look_at(dest, target_rotation) || ex.will_look_at(dest.below(), target_rotation) {
                state.set_input(Input::ClickRight, true);
            }
        }
    }
    match target_rotation {
        Some(r) => state.set_target(MovementTarget::new(r, true)),
        None => state.set_target(MovementTarget::new(to_dest, false)),
    };
    if feet == dest && (pos.y - (feet.y as f64) < 0.094 || is_water) {
        // 0.094 for lily pads.
        if is_water {
            // Only still water can be picked back up with a bucket.
            if let Some(empty) = ctx.inventory().find_in_hotbar("minecraft:bucket") {
                ex.inventory.select(empty);
                if v.y >= 0.0 {
                    state.set_input(Input::ClickRight, true);
                }
                return;
            } else if v.y >= 0.0 {
                state.set_status(MovementStatus::Success);
                return;
            }
            // Otherwise stay centred: this water might flow under the surface.
        } else {
            state.set_status(MovementStatus::Success);
            return;
        }
    }
    // Aim for the middle of the destination, not its edge (as falling onto a ladder would).
    let dest_center = dest.center();
    if (pos.x + v.x - dest_center.x).abs() > 0.1 || (pos.z + v.z - dest_center.z).abs() > 0.1 {
        if !ctx.player.on_ground && v.y.abs() > 0.4 {
            state.set_input(Input::Sneak, true);
        }
        state.set_input(Input::MoveForward, true);
    }
    let avoid = ladder_facing_below(ex, feet).map(|d| d.normal());
    let avoid = match avoid {
        None => {
            let d = src.subtract(dest);
            [d.x, d.y, d.z]
        }
        Some(a) => {
            let dist = (a[0] as f64 * (dest_center.x - a[0] as f64 / 2.0 - pos.x)).abs()
                + (a[2] as f64 * (dest_center.z - a[2] as f64 / 2.0 - pos.z)).abs();
            if dist < 0.6 {
                state.set_input(Input::MoveForward, true);
            } else if !ctx.player.on_ground {
                state.set_input(Input::Sneak, false);
            }
            a
        }
    };
    if target_rotation.is_none() {
        let offset = Vec3::new(
            dest_center.x + 0.125 * avoid[0] as f64,
            dest_center.y,
            dest_center.z + 0.125 * avoid[2] as f64,
        );
        state.set_target(MovementTarget::new(ctx.rotation_to(offset), false));
    }
}

/// `avoid`: the facing of a ladder up to 15 blocks below us, to fall onto its side rather than its back.
fn ladder_facing_below(ex: &Exec, feet: BlockPos) -> Option<Direction> {
    let world = &ex.ctx.world;
    for i in 0..15 {
        let info = world.info_pos(feet.below_n(i));
        if info.is(flags::LADDER) {
            return info.facing;
        }
    }
    None
}
