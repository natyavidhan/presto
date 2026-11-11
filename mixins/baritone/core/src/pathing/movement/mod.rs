//! Movements (`baritone.pathing.movement`): the eight ways Baritone gets from one block to the next, what each
//! costs (used by the search) and how each is carried out tick by tick (used by the path executor).

pub mod ascend;
pub mod descend;
pub mod diagonal;
pub mod downward;
pub mod fall;
pub mod moves;
pub mod parkour;
pub mod pillar;
pub mod traverse;

pub use moves::{MoveResult, Moves};

use super::context::CalculationContext;
use super::helper;
use crate::behavior::input::{Input, InputOverrideHandler};
use crate::behavior::inventory::{InvCtx, InventoryBehavior};
use crate::behavior::look::LookBehavior;
use crate::ctx::PlayerContext;
use crate::math::{BlockPos, Direction, Rotation, Vec3, mth, rotation_utils};
use crate::player::ChatLine;
use crate::world::HitResult;

/// `MovementStatus`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MovementStatus {
    /// Breaking what is in the way.
    Prepping,
    Waiting,
    Running,
    Success,
    /// The world changed and the movement can't be done any more.
    Unreachable,
    Failed,
    Canceled,
}

impl MovementStatus {
    pub fn is_complete(self) -> bool {
        matches!(
            self,
            MovementStatus::Success
                | MovementStatus::Unreachable
                | MovementStatus::Failed
                | MovementStatus::Canceled
        )
    }
}

/// `MovementState.MovementTarget`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MovementTarget {
    pub rotation: Option<Rotation>,
    /// The rotation matters to the crosshair (placing or breaking), not just to which way we walk.
    pub force_rotations: bool,
}

impl MovementTarget {
    pub fn new(rotation: Rotation, force_rotations: bool) -> Self {
        Self {
            rotation: Some(rotation),
            force_rotations,
        }
    }
}

/// `MovementState`.
#[derive(Clone, Debug)]
pub struct MovementState {
    pub status: MovementStatus,
    pub target: MovementTarget,
    inputs: Vec<(Input, bool)>,
}

impl Default for MovementState {
    fn default() -> Self {
        Self {
            status: MovementStatus::Prepping,
            target: MovementTarget::default(),
            inputs: Vec::new(),
        }
    }
}

impl MovementState {
    pub fn set_status(&mut self, status: MovementStatus) -> &mut Self {
        self.status = status;
        self
    }
    pub fn set_target(&mut self, target: MovementTarget) -> &mut Self {
        self.target = target;
        self
    }
    pub fn set_input(&mut self, input: Input, forced: bool) -> &mut Self {
        if let Some(e) = self.inputs.iter_mut().find(|(i, _)| *i == input) {
            e.1 = forced;
        } else {
            self.inputs.push((input, forced));
        }
        self
    }
    pub fn input(&self, input: Input) -> Option<bool> {
        self.inputs.iter().find(|(i, _)| *i == input).map(|e| e.1)
    }
}

/// What a movement may touch while it runs.
pub struct Exec<'a, 'b> {
    pub ctx: &'a PlayerContext<'b>,
    pub look: &'a mut LookBehavior,
    pub input: &'a mut InputOverrideHandler,
    pub inventory: &'a mut InventoryBehavior,
    /// `PathingBehavior.pathStart()`.
    pub path_start: BlockPos,
    /// The block the builder wants at a position (`BuilderProcess.placeAt`), for picking the right block.
    pub builder_place_at: &'a dyn Fn(i32, i32, i32) -> Option<String>,
    /// A menu other than the player's inventory is open.
    pub container_open: bool,
    /// `InventoryPauserProcess.stationaryForInventoryMove`.
    pub stationary: bool,
    /// Debug lines (`logDebug`).
    pub debug: &'a mut Vec<ChatLine>,
    /// Set when the movement wants creative flight off.
    pub stop_flying: bool,
}

impl Exec<'_, '_> {
    pub fn log_debug(&mut self, msg: impl Into<String>) {
        let msg = msg.into();
        log::debug!("[Baritone] {msg}");
        if self.ctx.settings.chat_debug {
            self.debug.push(ChatLine::plain(msg));
        }
    }

    pub fn inv_ctx(&self) -> InvCtx<'_> {
        InvCtx {
            inv: self.ctx.inventory(),
            settings: self.ctx.settings,
            container_open: self.container_open,
            stationary: self.stationary,
        }
    }

    /// `InventoryBehavior.selectThrowawayForLocation`.
    pub fn select_throwaway_for_location(&mut self, select: bool, x: i32, y: i32, z: i32) -> bool {
        let wanted = (self.builder_place_at)(x, y, z);
        let c = InvCtx {
            inv: self.ctx.inventory(),
            settings: self.ctx.settings,
            container_open: self.container_open,
            stationary: self.stationary,
        };
        self.inventory
            .select_throwaway_for_location(&c, select, wanted.as_deref())
    }

    pub fn has_generic_throwaway(&self) -> bool {
        crate::behavior::inventory::has_generic_throwaway(self.ctx.inventory(), self.ctx.settings)
    }

    /// `RotationUtils.reachable`, with the look behavior's aim.
    pub fn reachable(&self, pos: BlockPos, would_sneak: bool) -> Option<Rotation> {
        self.ctx
            .reachable(self.look.aim(), pos, self.ctx.block_reach_distance(), would_sneak)
    }

    pub fn reachable_with(&self, pos: BlockPos, reach: f64) -> Option<Rotation> {
        self.ctx.reachable(self.look.aim(), pos, reach, false)
    }

    /// `MovementHelper.switchToBestToolFor`.
    pub fn switch_to_best_tool_for(&mut self, pos: BlockPos) {
        let s = self.ctx.settings;
        if s.auto_tool && !s.assume_external_auto_tool {
            let info = self.ctx.world.info_pos(pos);
            let slot = self
                .ctx
                .world
                .tool_set
                .best_slot(info, s.prefer_silk_touch, false);
            self.inventory.select(slot);
        }
    }

    /// Whether the player is crouching (`isCrouching`).
    pub fn crouching(&self) -> bool {
        self.ctx.player.crouching
    }
}

/// `Movement`, with what each kind keeps between ticks.
#[derive(Clone, Debug)]
pub enum MovementKind {
    Traverse { was_the_bridge_block_always_there: bool },
    Ascend { ticks_without_placement: i32 },
    Descend { num_ticks: i32, force_safe_mode: bool },
    Diagonal,
    Pillar,
    Downward { num_ticks: i32 },
    Fall,
    Parkour { direction: Direction, dist: i32, ascend: bool },
}

impl MovementKind {
    pub fn name(&self) -> &'static str {
        match self {
            MovementKind::Traverse { .. } => "MovementTraverse",
            MovementKind::Ascend { .. } => "MovementAscend",
            MovementKind::Descend { .. } => "MovementDescend",
            MovementKind::Diagonal => "MovementDiagonal",
            MovementKind::Pillar => "MovementPillar",
            MovementKind::Downward { .. } => "MovementDownward",
            MovementKind::Fall => "MovementFall",
            MovementKind::Parkour { .. } => "MovementParkour",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Movement {
    pub kind: MovementKind,
    pub src: BlockPos,
    pub dest: BlockPos,
    /// The positions that need to be broken before this movement can ensue.
    pub positions_to_break: Vec<BlockPos>,
    /// The position where a block may need placing first.
    pub position_to_place: Option<BlockPos>,
    cost: Option<f64>,
    state: MovementState,
    valid_positions: Vec<BlockPos>,
    pub to_break_cached: Option<Vec<BlockPos>>,
    pub to_place_cached: Option<Vec<BlockPos>>,
    pub to_walk_into_cached: Option<Vec<BlockPos>>,
    calculated_while_loaded: bool,
}

impl Movement {
    pub(crate) fn new(
        kind: MovementKind,
        src: BlockPos,
        dest: BlockPos,
        positions_to_break: Vec<BlockPos>,
        position_to_place: Option<BlockPos>,
    ) -> Self {
        let mut m = Movement {
            kind,
            src,
            dest,
            positions_to_break,
            position_to_place,
            cost: None,
            state: MovementState::default(),
            valid_positions: Vec::new(),
            to_break_cached: None,
            to_place_cached: None,
            to_walk_into_cached: None,
            calculated_while_loaded: false,
        };
        m.valid_positions = m.calculate_valid_positions();
        m
    }

    pub fn traverse(src: BlockPos, dest: BlockPos) -> Self {
        Self::new(
            MovementKind::Traverse {
                was_the_bridge_block_always_there: true,
            },
            src,
            dest,
            vec![dest.above(), dest],
            Some(dest.below()),
        )
    }

    pub fn ascend(src: BlockPos, dest: BlockPos) -> Self {
        Self::new(
            MovementKind::Ascend {
                ticks_without_placement: 0,
            },
            src,
            dest,
            vec![dest, src.above_n(2), dest.above()],
            Some(dest.below()),
        )
    }

    pub fn descend(src: BlockPos, dest: BlockPos) -> Self {
        Self::new(
            MovementKind::Descend {
                num_ticks: 0,
                force_safe_mode: false,
            },
            src,
            dest,
            vec![dest.above_n(2), dest.above(), dest],
            Some(dest.below()),
        )
    }

    pub fn diagonal(src: BlockPos, dir1: Direction, dir2: Direction, dy: i32) -> Self {
        let d1 = src.relative(dir1);
        let d2 = src.relative(dir2);
        let end = d1.relative(dir2).above_n(dy);
        Self::new(
            MovementKind::Diagonal,
            src,
            end,
            vec![d1, d1.above(), d2, d2.above(), end, end.above()],
            None,
        )
    }

    pub fn pillar(src: BlockPos, dest: BlockPos) -> Self {
        Self::new(
            MovementKind::Pillar,
            src,
            dest,
            vec![src.above_n(2)],
            Some(src),
        )
    }

    pub fn downward(src: BlockPos, dest: BlockPos) -> Self {
        Self::new(
            MovementKind::Downward { num_ticks: 0 },
            src,
            dest,
            vec![dest],
            None,
        )
    }

    pub fn fall(src: BlockPos, dest: BlockPos) -> Self {
        Self::new(
            MovementKind::Fall,
            src,
            dest,
            fall::build_positions_to_break(src, dest),
            None,
        )
    }

    pub fn parkour(src: BlockPos, dist: i32, dir: Direction, ascend: bool) -> Self {
        let dest = src.relative_n(dir, dist).above_n(if ascend { 1 } else { 0 });
        let place = src.relative_n(dir, dist).below_n(if ascend { 0 } else { 1 });
        Self::new(
            MovementKind::Parkour {
                direction: dir,
                dist,
                ascend,
            },
            src,
            dest,
            Vec::new(),
            Some(place),
        )
    }

    /// The offset from `src` to `dest` (`getDirection`).
    pub fn direction(&self) -> BlockPos {
        self.dest.subtract(self.src)
    }

    pub fn status(&self) -> MovementStatus {
        self.state.status
    }

    /// The cost as cached (`getCost()`); panics on a movement never costed, as Java's would.
    pub fn cost(&self) -> f64 {
        self.cost.expect("movement cost was never calculated")
    }

    pub fn get_cost(&mut self, ctx: &CalculationContext) -> f64 {
        if let Some(c) = self.cost {
            return c;
        }
        let c = self.calculate_cost(ctx);
        self.cost = Some(c);
        c
    }

    pub fn recalculate_cost(&mut self, ctx: &CalculationContext) -> f64 {
        self.cost = None;
        self.get_cost(ctx)
    }

    pub fn override_cost(&mut self, cost: f64) {
        self.cost = Some(cost);
    }

    /// `calculateCost`: this exact movement's cost now (`COST_INF` if the search would end elsewhere).
    pub fn calculate_cost(&self, ctx: &CalculationContext) -> f64 {
        let (src, dest) = (self.src, self.dest);
        match &self.kind {
            MovementKind::Traverse { .. } => traverse::cost(ctx, src.x, src.y, src.z, dest.x, dest.z),
            MovementKind::Ascend { .. } => ascend::cost(ctx, src.x, src.y, src.z, dest.x, dest.z),
            MovementKind::Descend { .. } => {
                let mut res = MoveResult::new();
                descend::cost(ctx, src.x, src.y, src.z, dest.x, dest.z, &mut res);
                if res.y != dest.y {
                    super::costs::COST_INF // this position is a fall, not a descend
                } else {
                    res.cost
                }
            }
            MovementKind::Fall => {
                let mut res = MoveResult::new();
                descend::cost(ctx, src.x, src.y, src.z, dest.x, dest.z, &mut res);
                if res.y != dest.y {
                    super::costs::COST_INF // this position is a descend, not a fall
                } else {
                    res.cost
                }
            }
            MovementKind::Diagonal => {
                let mut res = MoveResult::new();
                diagonal::cost(ctx, src.x, src.y, src.z, dest.x, dest.z, &mut res);
                if res.y != dest.y {
                    super::costs::COST_INF
                } else {
                    res.cost
                }
            }
            MovementKind::Pillar => pillar::cost(ctx, src.x, src.y, src.z),
            MovementKind::Downward { .. } => downward::cost(ctx, src.x, src.y, src.z),
            MovementKind::Parkour { direction, .. } => {
                let mut res = MoveResult::new();
                parkour::cost(ctx, src.x, src.y, src.z, *direction, &mut res);
                if res.x != dest.x || res.y != dest.y || res.z != dest.z {
                    super::costs::COST_INF
                } else {
                    res.cost
                }
            }
        }
    }

    fn calculate_valid_positions(&self) -> Vec<BlockPos> {
        let (src, dest) = (self.src, self.dest);
        let mut v = match &self.kind {
            MovementKind::Traverse { .. } => vec![src, dest],
            MovementKind::Ascend { .. } => {
                // We sometimes back up to place the block; also sprint ascends, and descend-to-ascend skips.
                let prior = src.subtract(self.direction()).above();
                vec![src, src.above(), dest, prior, prior.above()]
            }
            MovementKind::Descend { .. } => vec![src, dest.above(), dest],
            MovementKind::Diagonal => {
                let diag_a = BlockPos::new(src.x, src.y, dest.z);
                let diag_b = BlockPos::new(dest.x, src.y, src.z);
                if dest.y < src.y {
                    vec![src, dest.above(), diag_a, diag_b, dest, diag_a.below(), diag_b.below()]
                } else if dest.y > src.y {
                    vec![src, src.above(), diag_a, diag_b, dest, diag_a.above(), diag_b.above()]
                } else {
                    vec![src, dest, diag_a, diag_b]
                }
            }
            MovementKind::Pillar | MovementKind::Downward { .. } => vec![src, dest],
            MovementKind::Fall => {
                let mut set = vec![src];
                let mut y = src.y - dest.y;
                while y >= 0 {
                    set.push(dest.above_n(y));
                    y -= 1;
                }
                set
            }
            MovementKind::Parkour { direction, dist, .. } => {
                let mut set = Vec::new();
                for i in 0..=*dist {
                    for y in 0..2 {
                        set.push(src.relative_n(*direction, i).above_n(y));
                    }
                }
                set
            }
        };
        v.sort();
        v.dedup();
        v
    }

    pub fn valid_positions(&self) -> &[BlockPos] {
        &self.valid_positions
    }

    /// `playerInValidPosition`.
    pub fn player_in_valid_position(&self, ex: &Exec) -> bool {
        let feet = ex.ctx.player_feet();
        self.valid_positions.contains(&feet) || self.valid_positions.contains(&ex.path_start)
    }

    /// `reset`.
    pub fn reset(&mut self) {
        self.state = MovementState::default();
        match &mut self.kind {
            MovementKind::Traverse {
                was_the_bridge_block_always_there,
            } => *was_the_bridge_block_always_there = true,
            MovementKind::Ascend {
                ticks_without_placement,
            } => *ticks_without_placement = 0,
            MovementKind::Descend {
                num_ticks,
                force_safe_mode,
            } => {
                *num_ticks = 0;
                *force_safe_mode = false;
            }
            MovementKind::Downward { num_ticks } => *num_ticks = 0,
            _ => {}
        }
    }

    pub fn check_loaded_chunk(&mut self, ctx: &CalculationContext) {
        self.calculated_while_loaded = ctx.bsi.world_contains_loaded_chunk(self.dest.x, self.dest.z);
    }

    pub fn calculated_while_loaded(&self) -> bool {
        self.calculated_while_loaded
    }

    pub fn reset_block_cache(&mut self) {
        self.to_break_cached = None;
        self.to_place_cached = None;
        self.to_walk_into_cached = None;
    }

    /// `toBreak`: what still has to be broken.
    pub fn to_break(&mut self, ctx: &CalculationContext) -> Vec<BlockPos> {
        if let Some(c) = &self.to_break_cached {
            return c.clone();
        }
        let range: &[BlockPos] = if matches!(self.kind, MovementKind::Diagonal) {
            &self.positions_to_break[4..6]
        } else {
            &self.positions_to_break
        };
        let result: Vec<BlockPos> = range
            .iter()
            .copied()
            .filter(|p| !helper::can_walk_through_pos(ctx, *p))
            .collect();
        self.to_break_cached = Some(result.clone());
        result
    }

    /// `toPlace`.
    pub fn to_place(&mut self, ctx: &CalculationContext) -> Vec<BlockPos> {
        if let Some(c) = &self.to_place_cached {
            return c.clone();
        }
        let mut result = Vec::new();
        if let Some(p) = self.position_to_place
            && !helper::can_walk_on_pos(ctx, p)
        {
            result.push(p);
        }
        self.to_place_cached = Some(result.clone());
        result
    }

    /// `toWalkInto`: only diagonals walk into blocks (the corners).
    pub fn to_walk_into(&mut self, ctx: &CalculationContext) -> Vec<BlockPos> {
        if let Some(c) = &self.to_walk_into_cached {
            return c.clone();
        }
        let result: Vec<BlockPos> = if matches!(self.kind, MovementKind::Diagonal) {
            self.positions_to_break[0..4]
                .iter()
                .copied()
                .filter(|p| !helper::can_walk_through_pos(ctx, *p))
                .collect()
        } else {
            Vec::new()
        };
        self.to_walk_into_cached = Some(result.clone());
        result
    }

    /// `safeToCancel`.
    pub fn safe_to_cancel(&self, ex: &Exec) -> bool {
        let state = &self.state;
        match &self.kind {
            MovementKind::Traverse { .. } => {
                state.status != MovementStatus::Running
                    || helper::can_walk_on_pos(&ex.ctx.world, self.dest.below())
            }
            MovementKind::Ascend {
                ticks_without_placement,
            } => state.status != MovementStatus::Running || *ticks_without_placement == 0,
            MovementKind::Diagonal => diagonal::safe_to_cancel(self, ex),
            MovementKind::Fall => {
                ex.ctx.player_feet() == self.src || state.status != MovementStatus::Running
            }
            MovementKind::Parkour { .. } => state.status != MovementStatus::Running,
            _ => true,
        }
    }

    /// `update`: one tick of the movement; sets the look target and the keys.
    pub fn update(&mut self, ex: &mut Exec) -> MovementStatus {
        ex.stop_flying = true;
        let mut state = std::mem::take(&mut self.state);
        self.update_state(&mut state, ex);
        let feet = ex.ctx.player_feet();
        if helper::is_liquid(ex.ctx.world.info_pos(feet))
            && ex.ctx.player.position.y < self.dest.y as f64 + 0.6
        {
            state.set_input(Input::Jump, true);
        }
        if ex.ctx.player.in_wall {
            if let Some(pos) = ex.ctx.selected_block() {
                ex.switch_to_best_tool_for(pos);
            }
            state.set_input(Input::ClickLeft, true);
        }
        if let Some(rotation) = state.target.rotation {
            ex.look.update_target(
                rotation,
                state.target.force_rotations,
                ex.ctx.settings,
                ex.ctx.player.fall_flying,
            );
        }
        ex.input.clear_all_keys();
        for (input, forced) in state.inputs.drain(..) {
            ex.input.set_input_force_state(input, forced);
        }
        if state.status.is_complete() {
            ex.input.clear_all_keys();
        }
        let status = state.status;
        self.state = state;
        status
    }

    /// `updateState`: the base class's, then the movement's own.
    fn update_state(&mut self, state: &mut MovementState, ex: &mut Exec) {
        match &self.kind {
            MovementKind::Traverse { .. } => traverse::update_state(self, state, ex),
            MovementKind::Ascend { .. } => ascend::update_state(self, state, ex),
            MovementKind::Descend { .. } => descend::update_state(self, state, ex),
            MovementKind::Diagonal => diagonal::update_state(self, state, ex),
            MovementKind::Pillar => pillar::update_state(self, state, ex),
            MovementKind::Downward { .. } => downward::update_state(self, state, ex),
            MovementKind::Fall => fall::update_state(self, state, ex),
            MovementKind::Parkour { .. } => parkour::update_state(self, state, ex),
        }
    }

    /// `Movement.updateState`, the base class's part: prepare, then go from prepping to running.
    pub(crate) fn base_update_state(&mut self, state: &mut MovementState, ex: &mut Exec) {
        let prepared = match &self.kind {
            MovementKind::Traverse { .. } => traverse::prepared(self, state, ex),
            MovementKind::Pillar => pillar::prepared(self, state, ex),
            MovementKind::Fall => fall::prepared(self, state, ex),
            MovementKind::Diagonal => true,
            _ => self.base_prepared(state, ex),
        };
        if !prepared {
            state.set_status(MovementStatus::Prepping);
            return;
        } else if state.status == MovementStatus::Prepping {
            state.set_status(MovementStatus::Waiting);
        }
        if state.status == MovementStatus::Waiting {
            state.set_status(MovementStatus::Running);
        }
    }

    /// `Movement.prepared`: break what is in the way.
    pub(crate) fn base_prepared(&self, state: &mut MovementState, ex: &mut Exec) -> bool {
        if state.status == MovementStatus::Waiting {
            return true;
        }
        for &pos in &self.positions_to_break {
            if ex.ctx.settings.pause_mining_for_falling_blocks
                && ex.ctx.game.entities.iter().any(|e| {
                    e.falling_block
                        && e.bounding_box.intersects(crate::math::Aabb::new(
                            pos.x as f64,
                            pos.y as f64,
                            pos.z as f64,
                            pos.x as f64 + 1.0,
                            pos.y as f64 + 1.1,
                            pos.z as f64 + 1.0,
                        ))
                })
            {
                return false;
            }
            if !helper::can_walk_through_pos(&ex.ctx.world, pos) {
                ex.switch_to_best_tool_for(pos);
                if let Some(rot) = ex.reachable(pos, false) {
                    state.set_target(MovementTarget::new(rot, true));
                    if ex.ctx.is_looking_at(pos) || ex.ctx.player_rotations().is_really_close_to(rot) {
                        state.set_input(Input::ClickLeft, true);
                    }
                    return false;
                }
                // No face of it is in sight: aim at its middle and break whatever is in the way anyway.
                // (Java Baritone then has an UNREACHABLE fallback that every path through here returns before.)
                state.set_target(MovementTarget::new(ex.ctx.rotation_to(pos.center()), true));
                state.set_input(Input::ClickLeft, true);
                return false;
            }
        }
        true
    }
}

/// `MovementHelper.moveTowards`: face the block (keeping the pitch) and walk forward.
pub fn move_towards(ex: &Exec, state: &mut MovementState, pos: BlockPos) {
    let rot = ex.ctx.rotation_to(pos.center()).with_pitch(ex.ctx.player_rotations().pitch);
    state.set_target(MovementTarget::new(rot, false));
    state.set_input(Input::MoveForward, true);
}

/// `MovementOption`: a combination of movement keys and the motion it gives.
struct MovementOption {
    input1: Input,
    input2: Option<Input>,
    motion_x: f32,
    motion_z: f32,
}

fn movement_options(mx: f32, mz: f32, can_sprint: bool) -> [MovementOption; 8] {
    const SPRINT_MULTIPLIER: f32 = 1.3;
    let (fx, fz) = if can_sprint {
        (mx * SPRINT_MULTIPLIER, mz * SPRINT_MULTIPLIER)
    } else {
        (mx, mz)
    };
    let o = |input1, input2, motion_x, motion_z| MovementOption {
        input1,
        input2,
        motion_x,
        motion_z,
    };
    [
        o(Input::MoveForward, None, fx, fz),
        o(Input::MoveBack, None, -mx, -mz),
        o(Input::MoveLeft, None, -mz, mx),
        o(Input::MoveRight, None, mz, -mx),
        o(Input::MoveForward, Some(Input::MoveLeft), fx - mz, fz + mx),
        o(Input::MoveForward, Some(Input::MoveRight), fx + mz, fz - mx),
        o(Input::MoveBack, Some(Input::MoveLeft), -mx - mz, -mz + mx),
        o(Input::MoveBack, Some(Input::MoveRight), -mx + mz, -mz - mx),
    ]
}

/// `moveTowardsWithoutRotation`: the keys that move closest to `ideal_yaw` without turning.
pub fn move_towards_without_rotation_yaw(ex: &Exec, state: &mut MovementState, ideal_yaw: f32) {
    let yaw = ex.ctx.player_rotations().yaw;
    let options = movement_options(
        mth::sin(yaw * rotation_utils::DEG_TO_RAD_F),
        mth::cos(yaw * rotation_utils::DEG_TO_RAD_F),
        ex.ctx.settings.allow_sprint,
    );
    let tx = mth::sin(ideal_yaw * rotation_utils::DEG_TO_RAD_F);
    let tz = mth::cos(ideal_yaw * rotation_utils::DEG_TO_RAD_F);
    let best = options.iter().min_by(|a, b| {
        let da = (a.motion_x - tx).abs() + (a.motion_z - tz).abs();
        let db = (b.motion_x - tx).abs() + (b.motion_z - tz).abs();
        da.total_cmp(&db)
    });
    if let Some(sel) = best {
        state.set_input(sel.input1, true);
        if let Some(i2) = sel.input2 {
            state.set_input(i2, true);
        }
    }
}

pub fn move_towards_without_rotation(ex: &Exec, state: &mut MovementState, dest: BlockPos) {
    let ideal = ex.ctx.rotation_to(dest.center()).yaw;
    move_towards_without_rotation_yaw(ex, state, ideal);
}

/// `moveTowardsWithSlightRotation`: turn to the nearest 45 degrees and strafe the rest.
pub fn move_towards_with_slight_rotation(ex: &Exec, state: &mut MovementState, dest: BlockPos) {
    let ideal = ex.ctx.rotation_to(dest.center()).yaw;
    let current = ex.ctx.player_rotations();
    let distance = Rotation::yaw_distance_from_offset(current.yaw, ideal) % 45.0;
    let new_yaw = if distance > 0.0 {
        if distance > 22.5 { distance - 45.0 } else { distance }
    } else if distance < -22.5 {
        distance + 45.0
    } else {
        distance
    };
    state.set_target(MovementTarget::new(
        Rotation::new(current.yaw - new_yaw, current.pitch),
        true,
    ));
    move_towards_without_rotation_yaw(ex, state, ideal);
}

/// `MovementHelper.PlaceResult`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaceResult {
    ReadyToPlace,
    Attempting,
    NoOption,
}

/// `MovementHelper.attemptToPlaceABlock`.
pub fn attempt_to_place_a_block(
    state: &mut MovementState,
    ex: &mut Exec,
    place_at: BlockPos,
    prefer_down: bool,
    would_sneak: bool,
) -> PlaceResult {
    // If there is a block there, it must be replaceable.
    let direct = ex.reachable(place_at, would_sneak);
    let mut found = false;
    if let Some(rot) = direct {
        state.set_target(MovementTarget::new(rot, true));
        found = true;
    }
    for dir in Direction::HORIZONTALS_AND_DOWN {
        let against = place_at.relative(dir);
        if !helper::can_place_against_pos(&ex.ctx.world, against) {
            continue;
        }
        if !ex.select_throwaway_for_location(false, place_at.x, place_at.y, place_at.z) {
            ex.log_debug("bb pls get me some blocks. dirt, netherrack, cobble");
            state.set_status(MovementStatus::Unreachable);
            return PlaceResult::NoOption;
        }
        let face = Vec3::new(
            (place_at.x + against.x) as f64 * 0.5 + 0.5,
            (place_at.y + against.y) as f64 * 0.5 + 0.25,
            (place_at.z + against.z) as f64 * 0.5 + 0.5,
        );
        let eyes = if would_sneak {
            ex.ctx.player.sneaking_eyes()
        } else {
            ex.ctx.player_head()
        };
        let place = rotation_utils::calc_rotation_from_vec3d_rel(eyes, face, ex.ctx.player_rotations());
        let actual = ex
            .look
            .peek(place, ex.ctx.player_rotations(), ex.ctx.game.mouse_sensitivity);
        let res = ex
            .ctx
            .ray_trace_towards(actual, ex.ctx.block_reach_distance(), would_sneak);
        if let HitResult::Block { pos, face, .. } = res
            && pos == against
            && pos.relative(face) == place_at
        {
            state.set_target(MovementTarget::new(place, true));
            found = true;
            if !prefer_down {
                // With prefer_down we want the last option (down); otherwise the first.
                break;
            }
        }
    }
    if let HitResult::Block { pos: selected, face, .. } = ex.ctx.object_mouse_over() {
        // Only a replaceable block can be the very block we are placing at.
        if selected == place_at
            || (helper::can_place_against_pos(&ex.ctx.world, selected) && selected.relative(face) == place_at)
        {
            if would_sneak {
                state.set_input(Input::Sneak, true);
            }
            ex.select_throwaway_for_location(true, place_at.x, place_at.y, place_at.z);
            return PlaceResult::ReadyToPlace;
        }
    }
    if found {
        if would_sneak {
            state.set_input(Input::Sneak, true);
        }
        ex.select_throwaway_for_location(true, place_at.x, place_at.y, place_at.z);
        return PlaceResult::Attempting;
    }
    PlaceResult::NoOption
}

/// `MovementHelper.isDoorPassable`, `isGatePassable` from the player's side.
pub fn door_passable(ex: &Exec, door: BlockPos, player: BlockPos) -> bool {
    helper::is_door_passable(&ex.ctx.world, door, player)
}

pub fn gate_passable(ex: &Exec, gate: BlockPos, player: BlockPos) -> bool {
    helper::is_gate_passable(&ex.ctx.world, gate, player)
}
