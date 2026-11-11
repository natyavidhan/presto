//! `MovementDownward`: straight down one block, mining what we stand on or climbing down a ladder.

use super::{Exec, Movement, MovementKind, MovementState, MovementStatus, move_towards};
use crate::pathing::context::CalculationContext;
use crate::pathing::costs::*;
use crate::pathing::helper::*;
use crate::world::flags;

pub fn cost(ctx: &CalculationContext, x: i32, y: i32, z: i32) -> f64 {
    if !ctx.allow_downward {
        return COST_INF;
    }
    if !can_walk_on(ctx, x, y - 2, z) {
        return COST_INF;
    }
    let down_state = ctx.get(x, y - 1, z);
    let down = ctx.table.get(down_state);
    if down.is(flags::LADDER) || down.is(flags::VINE) {
        LADDER_DOWN_ONE_COST
    } else {
        // We stand on it: it may be a falling block, but it'll be air by the time we're there.
        fall_n_blocks_cost(1) + get_mining_duration_ticks_state(ctx, x, y - 1, z, down_state, false)
    }
}

pub(super) fn update_state(m: &mut Movement, state: &mut MovementState, ex: &mut Exec) {
    m.base_update_state(state, ex);
    if state.status != MovementStatus::Running {
        return;
    }
    let ctx = ex.ctx;
    let feet = ctx.player_feet();
    if feet == m.dest {
        state.set_status(MovementStatus::Success);
        return;
    } else if !m.player_in_valid_position(ex) {
        state.set_status(MovementStatus::Unreachable);
        return;
    }
    let pos = ctx.player.position;
    let diff_x = pos.x - (m.dest.x as f64 + 0.5);
    let diff_z = pos.z - (m.dest.z as f64 + 0.5);
    let ab = (diff_x * diff_x + diff_z * diff_z).sqrt();
    let num_ticks = if let MovementKind::Downward { num_ticks } = &mut m.kind {
        let n = *num_ticks;
        *num_ticks += 1;
        n
    } else {
        0
    };
    if num_ticks < 10 && ab < 0.2 {
        return;
    }
    let target = m.positions_to_break[0];
    move_towards(ex, state, target);
}
