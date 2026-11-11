//! `PrecomputedData`: the per-state halves of `canWalkOn`, `canWalkThrough` and `fullyPassable`, worked out
//! once per state and kept, so the search asks the position-dependent half only for the few states that need
//! it (`Ternary.MAYBE`).

use super::helper;
use crate::world::{BlockTable, StateId};
use std::cell::Cell;

/// `Ternary`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ternary {
    Yes,
    Maybe,
    No,
}

const COMPLETED: u8 = 1 << 0;
const FULLY_PASSABLE_MAYBE: u8 = 1 << 1;
const FULLY_PASSABLE: u8 = 1 << 2;
const CAN_WALK_THROUGH_MAYBE: u8 = 1 << 3;
const CAN_WALK_THROUGH: u8 = 1 << 4;
const CAN_WALK_ON_MAYBE: u8 = 1 << 5;
const CAN_WALK_ON: u8 = 1 << 6;

/// The per-state answers, filled in on first use. One calculation owns one; it is not shared between threads.
pub struct PrecomputedData {
    data: Vec<Cell<u8>>,
}

impl PrecomputedData {
    pub fn new(table: &BlockTable) -> Self {
        Self {
            data: (0..table.len()).map(|_| Cell::new(0)).collect(),
        }
    }

    #[inline]
    fn get(&self, ctx: &super::context::CalculationContext, state: StateId) -> u8 {
        let Some(cell) = self.data.get(state.0 as usize) else {
            return Self::compute(ctx, state);
        };
        let d = cell.get();
        if d & COMPLETED != 0 {
            return d;
        }
        let d = Self::compute(ctx, state);
        cell.set(d);
        d
    }

    fn compute(ctx: &super::context::CalculationContext, state: StateId) -> u8 {
        let info = ctx.table.get(state);
        let mut d = COMPLETED;
        match helper::can_walk_on_block_state(ctx, info) {
            Ternary::Yes => d |= CAN_WALK_ON,
            Ternary::Maybe => d |= CAN_WALK_ON_MAYBE,
            Ternary::No => {}
        }
        match helper::can_walk_through_block_state(ctx, info) {
            Ternary::Yes => d |= CAN_WALK_THROUGH,
            Ternary::Maybe => d |= CAN_WALK_THROUGH_MAYBE,
            Ternary::No => {}
        }
        match helper::fully_passable_block_state(info) {
            Ternary::Yes => d |= FULLY_PASSABLE,
            Ternary::Maybe => d |= FULLY_PASSABLE_MAYBE,
            Ternary::No => {}
        }
        d
    }

    #[inline]
    pub fn can_walk_on(
        &self,
        ctx: &super::context::CalculationContext,
        x: i32,
        y: i32,
        z: i32,
        state: StateId,
    ) -> bool {
        let d = self.get(ctx, state);
        if d & CAN_WALK_ON_MAYBE != 0 {
            helper::can_walk_on_position(ctx, x, y, z, ctx.table.get(state))
        } else {
            d & CAN_WALK_ON != 0
        }
    }

    #[inline]
    pub fn can_walk_through(
        &self,
        ctx: &super::context::CalculationContext,
        x: i32,
        y: i32,
        z: i32,
        state: StateId,
    ) -> bool {
        let d = self.get(ctx, state);
        if d & CAN_WALK_THROUGH_MAYBE != 0 {
            helper::can_walk_through_position(ctx, x, y, z, ctx.table.get(state))
        } else {
            d & CAN_WALK_THROUGH != 0
        }
    }

    #[inline]
    pub fn fully_passable(&self, ctx: &super::context::CalculationContext, state: StateId) -> bool {
        let d = self.get(ctx, state);
        if d & FULLY_PASSABLE_MAYBE != 0 {
            // `fullyPassablePosition` keeps its parameters only for compatibility.
            ctx.table
                .get(state)
                .is(crate::world::flags::PATHFINDABLE_LAND)
        } else {
            d & FULLY_PASSABLE != 0
        }
    }
}
