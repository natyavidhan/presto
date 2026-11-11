//! `PathExecutor`: walks a path one movement at a time, noticing when the world or the player has strayed
//! from the plan, and taking the shortcuts Presto takes (sprinting through descends into ascends,
//! skipping a traverse into a straight ascend, running past a fall onto what's after it).

use super::costs::COST_INF;
use super::helper::*;
use super::movement::{Exec, MovementKind, MovementStatus, descend};
use super::path::Path;
use crate::behavior::input::Input;
use crate::math::{BlockPos, Vec3, distance_to_center};
use rustc_hash::FxHashSet;

const MAX_MAX_DIST_FROM_PATH: f64 = 3.0;
const MAX_DIST_FROM_PATH: f64 = 2.0;
/// Ten seconds; at least 5.5 (110 ticks), issue #102.
const MAX_TICKS_AWAY: i32 = 200;

#[derive(Clone, Debug)]
pub struct PathExecutor {
    path: Path,
    path_position: usize,
    ticks_away: i32,
    ticks_on_current: i32,
    current_movement_original_cost_estimate: f64,
    cost_estimate_index: Option<usize>,
    failed: bool,
    recalc_bp: bool,
    to_break: FxHashSet<BlockPos>,
    to_place: FxHashSet<BlockPos>,
    to_walk_into: FxHashSet<BlockPos>,
    sprint_next_tick: bool,
}

impl PathExecutor {
    pub fn new(path: Path) -> Self {
        Self {
            path,
            path_position: 0,
            ticks_away: 0,
            ticks_on_current: 0,
            current_movement_original_cost_estimate: 0.0,
            cost_estimate_index: None,
            failed: false,
            recalc_bp: true,
            to_break: FxHashSet::default(),
            to_place: FxHashSet::default(),
            to_walk_into: FxHashSet::default(),
            sprint_next_tick: false,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn position(&self) -> usize {
        self.path_position
    }
    pub fn failed(&self) -> bool {
        self.failed
    }
    pub fn finished(&self) -> bool {
        self.path_position >= self.path.len()
    }
    pub fn is_sprinting(&self) -> bool {
        self.sprint_next_tick
    }
    pub fn to_break(&self) -> &FxHashSet<BlockPos> {
        &self.to_break
    }
    pub fn to_place(&self) -> &FxHashSet<BlockPos> {
        &self.to_place
    }
    pub fn to_walk_into(&self) -> &FxHashSet<BlockPos> {
        &self.to_walk_into
    }

    /// Whether the current movement may be dropped right now (`movement.safeToCancel()`).
    pub fn current_safe_to_cancel(&self, ex: &Exec) -> bool {
        self.path
            .movements()
            .get(self.path_position)
            .is_none_or(|m| m.safe_to_cancel(ex))
    }

    /// `onTick`: true when a movement just finished, or the current one can be cancelled (the player is in
    /// a stable state, not sneaking out over lava). `in_progress_best` is the running search's best path so
    /// far, for pausing on a backtrack.
    pub fn on_tick(&mut self, ex: &mut Exec, in_progress_best: Option<&[BlockPos]>) -> bool {
        if self.path_position + 1 == self.path.len() {
            self.path_position += 1;
        }
        if self.path_position >= self.path.len() {
            return true; // done
        }
        let where_am_i = ex.ctx.player_feet();
        if !self.path.movements()[self.path_position]
            .valid_positions()
            .contains(&where_am_i)
        {
            // Lagged back a few blocks?
            for i in 0..self.path_position.min(self.path.len()) {
                if self.path.movements()[i].valid_positions().contains(&where_am_i) {
                    let previous = self.path_position;
                    self.path_position = i;
                    for j in self.path_position..=previous.min(self.path.movements().len() - 1) {
                        self.path.movements_mut()[j].reset();
                    }
                    self.on_change_in_path_position(ex);
                    self.on_tick(ex, in_progress_best);
                    return false;
                }
            }
            // Ahead of the plan? (Not +1: the movement says when it's done, as sneak placing does. Not +2.)
            for i in self.path_position + 3..self.path.len().saturating_sub(1) {
                if self.path.movements()[i].valid_positions().contains(&where_am_i) {
                    if i - self.path_position > 2 {
                        ex.log_debug(format!(
                            "Skipping forward {} steps, to {i}",
                            i - self.path_position
                        ));
                    }
                    self.path_position = i - 1;
                    self.on_change_in_path_position(ex);
                    self.on_tick(ex, in_progress_best);
                    return false;
                }
            }
        }
        let (dist, _) = self.closest_path_pos(ex);
        if self.possibly_off_path(ex, dist, MAX_DIST_FROM_PATH) {
            self.ticks_away += 1;
            log::debug!(
                "[Presto] FAR AWAY FROM PATH FOR {} TICKS. Current distance: {dist}. Threshold: {MAX_DIST_FROM_PATH}",
                self.ticks_away
            );
            if self.ticks_away > MAX_TICKS_AWAY {
                ex.log_debug("Too far away from path for too long, cancelling path");
                self.cancel(ex);
                return false;
            }
        } else {
            self.ticks_away = 0;
        }
        if self.possibly_off_path(ex, dist, MAX_MAX_DIST_FROM_PATH) {
            ex.log_debug("too far from path");
            self.cancel(ex);
            return false;
        }
        let ctx = ex.ctx;
        let world = &ctx.world;
        let lo = self.path_position.saturating_sub(10);
        let hi = (self.path_position + 10).min(self.path.movements().len());
        for i in lo..hi {
            let m = &mut self.path.movements_mut()[i];
            let prev_break = m.to_break(world);
            let prev_place = m.to_place(world);
            let prev_walk = m.to_walk_into(world);
            m.reset_block_cache();
            if prev_break != m.to_break(world) || prev_place != m.to_place(world) || prev_walk != m.to_walk_into(world) {
                self.recalc_bp = true;
            }
        }
        if self.recalc_bp {
            let mut nb = FxHashSet::default();
            let mut np = FxHashSet::default();
            let mut nw = FxHashSet::default();
            for i in self.path_position..self.path.movements().len() {
                let m = &mut self.path.movements_mut()[i];
                nb.extend(m.to_break(world));
                np.extend(m.to_place(world));
                nw.extend(m.to_walk_into(world));
            }
            self.to_break = nb;
            self.to_place = np;
            self.to_walk_into = nw;
            self.recalc_bp = false;
        }
        if self.path_position + 1 < self.path.movements().len() {
            let next_dest = self.path.movements()[self.path_position + 1].dest;
            if !world.bsi.world_contains_loaded_chunk(next_dest.x, next_dest.z) {
                ex.log_debug("Pausing since destination is at edge of loaded chunks");
                ex.input.clear_all_keys();
                return true;
            }
        }
        let can_cancel = self.path.movements()[self.path_position].safe_to_cancel(ex);
        if self.cost_estimate_index != Some(self.path_position) {
            self.cost_estimate_index = Some(self.path_position);
            // Once, when the movement starts: the cost as it was costed when the path was found.
            self.current_movement_original_cost_estimate = self.path.movements()[self.path_position].cost();
            let lookahead = ex.ctx.settings.cost_verification_lookahead.max(0) as usize;
            for i in 1..lookahead {
                if self.path_position + i >= self.path.len() - 1 {
                    break;
                }
                if self.path.movements()[self.path_position + i].calculate_cost(world) >= COST_INF && can_cancel {
                    ex.log_debug("Something has changed in the world and a future movement has become impossible. Cancelling.");
                    self.cancel(ex);
                    return true;
                }
            }
        }
        let current_cost = self.path.movements_mut()[self.path_position].recalculate_cost(world);
        if current_cost >= COST_INF && can_cancel {
            ex.log_debug("Something has changed in the world and this movement has become impossible. Cancelling.");
            self.cancel(ex);
            return true;
        }
        if !self.path.movements()[self.path_position].calculated_while_loaded()
            && current_cost - self.current_movement_original_cost_estimate > ex.ctx.settings.max_cost_increase
            && can_cancel
        {
            // Not for a movement costed while loaded: that's the path interfering with itself, not a stale cache.
            ex.log_debug(format!(
                "Original cost {} current cost {current_cost}. Cancelling.",
                self.current_movement_original_cost_estimate
            ));
            self.cancel(ex);
            return true;
        }
        if self.should_pause(ex, in_progress_best) {
            ex.log_debug("Pausing since current best path is a backtrack");
            ex.input.clear_all_keys();
            return true;
        }
        let status = self.path.movements_mut()[self.path_position].update(ex);
        match status {
            MovementStatus::Unreachable | MovementStatus::Failed => {
                ex.log_debug(format!("Movement returns status {status:?}"));
                self.cancel(ex);
                return true;
            }
            MovementStatus::Success => {
                self.path_position += 1;
                self.on_change_in_path_position(ex);
                self.on_tick(ex, in_progress_best);
                return true;
            }
            _ => {}
        }
        self.sprint_next_tick = self.should_sprint_next_tick(ex, in_progress_best);
        self.ticks_on_current += 1;
        if self.ticks_on_current as f64
            > self.current_movement_original_cost_estimate + ex.ctx.settings.movement_timeout_ticks as f64
        {
            // Compared with the first estimate: breaking blocks lowers the current cost as we go.
            ex.log_debug(format!(
                "This movement has taken too long ({} ticks, expected {}). Cancelling.",
                self.ticks_on_current, self.current_movement_original_cost_estimate
            ));
            self.cancel(ex);
            return true;
        }
        can_cancel // in progress, but PathingBehavior may cut onto the next path if this is cancellable
    }

    /// `closestPathPos`: the nearest valid position of any movement.
    fn closest_path_pos(&self, ex: &Exec) -> (f64, Option<BlockPos>) {
        let p = ex.ctx.player.position;
        let mut best = -1.0;
        let mut best_pos = None;
        for m in self.path.movements() {
            for pos in m.valid_positions() {
                let d = distance_to_center(*pos, p.x, p.y, p.z);
                if d < best || best == -1.0 {
                    best = d;
                    best_pos = Some(*pos);
                }
            }
        }
        (best, best_pos)
    }

    /// `shouldPause`: the search running for the next segment found a better path that goes back through
    /// where we are; wait for it rather than walk away.
    fn should_pause(&self, ex: &Exec, in_progress_best: Option<&[BlockPos]>) -> bool {
        let Some(best) = in_progress_best else {
            return false;
        };
        if !ex.ctx.player.on_ground {
            return false;
        }
        let feet = ex.ctx.player_feet();
        let world = &ex.ctx.world;
        if !can_walk_on_pos(world, feet.below()) {
            return false; // sketchy, maybe parkouring
        }
        if !can_walk_through_pos(world, feet) || !can_walk_through_pos(world, feet.above()) {
            return false; // suffocating?
        }
        if !self.path.movements()[self.path_position].safe_to_cancel(ex) {
            return false;
        }
        if best.len() < 3 {
            return false; // too short to be sure we'll take it
        }
        // The next path's first block always overlaps.
        best[1..].contains(&feet)
    }

    /// `possiblyOffPath`: mid-fall we're far from both ends without being off the path.
    fn possibly_off_path(&self, ex: &Exec, distance: f64, leniency: f64) -> bool {
        if distance <= leniency {
            return false;
        }
        if matches!(
            self.path.movements().get(self.path_position).map(|m| &m.kind),
            Some(MovementKind::Fall)
        ) {
            let fall_dest = self.path.positions()[self.path_position + 1];
            let p = ex.ctx.player.position;
            return distance_to_center(fall_dest, p.x, fall_dest.y as f64 + 0.5, p.z) >= leniency;
        }
        true
    }

    /// `snipsnapifpossible`: jump to where the player stands on this path, if it is safe to switch now.
    pub fn snipsnap_if_possible(&mut self, ex: &mut Exec) -> bool {
        let feet = ex.ctx.player_feet();
        if !ex.ctx.player.on_ground && !ex.ctx.world.info_pos(feet).has_fluid() {
            return false; // falling through the air
        }
        if ex.ctx.player.velocity.y < -0.1 {
            return false; // maybe falling through water
        }
        let Some(index) = self.path.index_of(feet) else {
            return false;
        };
        self.path_position = index;
        ex.input.clear_all_keys();
        true
    }

    fn should_sprint_next_tick(&mut self, ex: &mut Exec, in_progress_best: Option<&[BlockPos]>) -> bool {
        let requested = ex.input.is_input_forced_down(Input::Sprint);
        // From here on we decide; the game shouldn't sprint for us.
        ex.input.set_input_force_state(Input::Sprint, false);
        if !ex.ctx.world.can_sprint {
            return false;
        }
        let ctx = ex.ctx;
        let world = &ctx.world;
        let pos = self.path_position;
        let len = self.path.len();
        // A traverse asks to sprint, so check it before the request.
        if matches!(kind_name(&self.path, pos), Some("MovementTraverse"))
            && pos + 3 < len
            && matches!(kind_name(&self.path, pos + 1), Some("MovementAscend"))
            && self.sprintable_ascend(ex, pos, pos + 1, pos + 2)
        {
            if skip_now(ex, &self.path.movements()[pos]) {
                ex.log_debug("Skipping traverse to straight ascend");
                self.path_position += 1;
                self.on_change_in_path_position(ex);
                self.on_tick(ex, in_progress_best);
                ex.input.set_input_force_state(Input::Jump, true);
                return true;
            } else {
                ex.log_debug("Too far to the side to safely sprint ascend");
            }
        }
        if requested {
            return true;
        }
        if matches!(kind_name(&self.path, pos), Some("MovementDescend")) {
            if pos + 2 < len {
                let next = &self.path.movements()[pos + 1];
                if can_use_frost_walker(world, world.info_pos(next.dest.below()))
                    && matches!(next.kind, MovementKind::Traverse { .. } | MovementKind::Parkour { .. })
                {
                    // Frost Walker only freezes what we cross on the ground, so we may not overshoot.
                    let could_place_instead = ex.ctx.settings.allow_place
                        && ex.has_generic_throwaway()
                        && matches!(next.kind, MovementKind::Parkour { .. }); // a traverse can't react in time
                    let cur = dir_of(&self.path, pos).above();
                    let nd = next.direction();
                    let sum = cur.add(nd);
                    let cross = BlockPos::new(
                        cur.y * nd.z - cur.z * nd.y,
                        cur.z * nd.x - cur.x * nd.z,
                        cur.x * nd.y - cur.y * nd.x,
                    );
                    let same_flat_direction = sum != BlockPos::ORIGIN && cross == BlockPos::ORIGIN;
                    if same_flat_direction && !could_place_instead {
                        descend::force_safe_mode(&mut self.path.movements_mut()[pos]);
                    }
                }
            }
            let current = &self.path.movements()[pos];
            if descend::safe_mode(current, world) && !descend::skip_to_ascend(current, world) {
                ex.log_debug("Sprinting would be unsafe");
                return false;
            }
            if pos + 2 < len {
                let next_kind = kind_name(&self.path, pos + 1);
                if next_kind == Some("MovementAscend") && dir_of(&self.path, pos).above() == dir_of(&self.path, pos + 1).below() {
                    // A descend, then an ascend the same way.
                    self.path_position += 1;
                    self.on_change_in_path_position(ex);
                    self.on_tick(ex, in_progress_best);
                    ex.log_debug("Skipping descend to straight ascend");
                    return true;
                }
                if self.can_sprint_from_descend_into(ex, pos, pos + 1) {
                    if matches!(kind_name(&self.path, pos + 1), Some("MovementDescend"))
                        && pos + 3 < len
                        && matches!(kind_name(&self.path, pos + 2), Some("MovementDescend"))
                        && !self.can_sprint_from_descend_into(ex, pos + 1, pos + 2)
                    {
                        return false;
                    }
                    if ex.ctx.player_feet() == self.path.movements()[pos].dest {
                        self.path_position += 1;
                        self.on_change_in_path_position(ex);
                        self.on_tick(ex, in_progress_best);
                    }
                    return true;
                }
            }
        }
        if matches!(kind_name(&self.path, pos), Some("MovementAscend")) && pos != 0 {
            let prev_kind = kind_name(&self.path, pos - 1);
            if prev_kind == Some("MovementDescend") && dir_of(&self.path, pos - 1).above() == dir_of(&self.path, pos).below() {
                let center = self.path.movements()[pos].src.above();
                // playerFeet adds 0.1251 for soul sand; farmland is 0.9375, hence 0.07.
                if ex.ctx.player.position.y >= center.y as f64 - 0.07 {
                    ex.input.set_input_force_state(Input::Jump, false);
                    return true;
                }
            }
            if pos + 2 < len && prev_kind == Some("MovementTraverse") && self.sprintable_ascend(ex, pos - 1, pos, pos + 1) {
                return true;
            }
        }
        if matches!(kind_name(&self.path, pos), Some("MovementFall"))
            && let Some((target, fall_dest)) = self.override_fall(ex)
        {
            assert!(
                self.path.contains(fall_dest),
                "Fall override returned illegal destination {fall_dest:?}"
            );
            if ex.ctx.player_feet() == fall_dest {
                self.path_position = self.path.index_of(fall_dest).unwrap_or(self.path_position);
                self.on_change_in_path_position(ex);
                self.on_tick(ex, in_progress_best);
                return true;
            }
            ex.input.clear_all_keys();
            let rot = ex.ctx.rotation_to(target);
            ex.look.update_target(rot, false, ex.ctx.settings, ex.ctx.player.fall_flying);
            ex.input.set_input_force_state(Input::MoveForward, true);
            return true;
        }
        false
    }

    /// `overrideFall`: the traverses after a fall continue the same way, so aim past them while falling.
    fn override_fall(&mut self, ex: &Exec) -> Option<(Vec3, BlockPos)> {
        let pos = self.path_position;
        let world = &ex.ctx.world;
        let movement = &self.path.movements()[pos];
        let dir = movement.direction();
        if dir.y < -3 {
            return None;
        }
        if !movement.to_break_cached.as_ref().is_none_or(|v| v.is_empty()) {
            return None; // it's breaking
        }
        let flat = BlockPos::new(dir.x, 0, dir.z);
        let len = self.path.len();
        let mut i = pos + 1;
        'outer: while i < len - 1 && i < pos + 3 {
            let next = &self.path.movements()[i];
            if !matches!(next.kind, MovementKind::Traverse { .. }) {
                break;
            }
            if flat != next.direction() {
                break;
            }
            for y in next.dest.y..=movement.src.y + 1 {
                if !fully_passable(world, next.dest.x, y, next.dest.z) {
                    break 'outer;
                }
            }
            if !can_walk_on_pos(world, next.dest.below()) {
                break;
            }
            i += 1;
        }
        i -= 1;
        if i == pos {
            return None; // no valid extension
        }
        let len = (i - pos) as f64 - 0.4;
        let dest = movement.dest;
        Some((
            Vec3::new(
                flat.x as f64 * len + dest.x as f64 + 0.5,
                dest.y as f64,
                flat.z as f64 * len + dest.z as f64 + 0.5,
            ),
            dest.offset(flat.x * (i - pos) as i32, 0, flat.z * (i - pos) as i32),
        ))
    }

    /// `sprintableAscend`.
    fn sprintable_ascend(&self, ex: &Exec, current: usize, next: usize, next_next: usize) -> bool {
        if !ex.ctx.settings.sprint_ascends {
            return false;
        }
        let world = &ex.ctx.world;
        let (Some(cur), Some(nx), Some(nn)) = (
            self.path.movements().get(current),
            self.path.movements().get(next),
            self.path.movements().get(next_next),
        ) else {
            return false;
        };
        if cur.direction() != nx.direction().below() {
            return false;
        }
        if nn.direction().x != nx.direction().x || nn.direction().z != nx.direction().z {
            return false;
        }
        if !can_walk_on_pos(world, cur.dest.below()) {
            return false;
        }
        if !can_walk_on_pos(world, nx.dest.below()) {
            return false;
        }
        if !nx.to_break_cached.as_ref().is_none_or(|v| v.is_empty()) {
            return false; // it's breaking
        }
        for x in 0..2 {
            for y in 0..3 {
                let mut chk = cur.src.above_n(y);
                if x == 1 {
                    chk = chk.add(cur.direction());
                }
                if !fully_passable(world, chk.x, chk.y, chk.z) {
                    return false;
                }
            }
        }
        if avoid_walking_into(world, world.info_pos(cur.src.above_n(3))) {
            return false;
        }
        !avoid_walking_into(world, world.info_pos(nx.dest.above_n(2)))
    }

    /// `canSprintFromDescendInto`.
    fn can_sprint_from_descend_into(&self, ex: &Exec, current: usize, next: usize) -> bool {
        let world = &ex.ctx.world;
        let cur = &self.path.movements()[current];
        let nx = &self.path.movements()[next];
        if matches!(nx.kind, MovementKind::Descend { .. }) && nx.direction() == cur.direction() {
            return true;
        }
        if !can_walk_on_pos(world, cur.dest.add(cur.direction())) {
            return false;
        }
        if matches!(nx.kind, MovementKind::Traverse { .. }) && nx.direction() == cur.direction() {
            return true;
        }
        matches!(nx.kind, MovementKind::Diagonal) && ex.ctx.settings.allow_overshoot_diagonal_descend
    }

    fn on_change_in_path_position(&mut self, ex: &mut Exec) {
        ex.input.clear_all_keys();
        self.ticks_on_current = 0;
    }

    fn cancel(&mut self, ex: &mut Exec) {
        ex.input.clear_all_keys();
        ex.input.break_helper.stop_breaking_block();
        self.path_position = self.path.len() + 3;
        self.failed = true;
    }

    /// `trySplice`: join the next path on now if possible, else cut the start off a very long one.
    pub fn try_splice(&self, next: Option<&PathExecutor>, max_history: i32, cutoff_amount: i32) -> Option<PathExecutor> {
        let Some(next) = next else {
            return self.cut_if_too_long(max_history, cutoff_amount);
        };
        match Path::try_splice(&self.path, &next.path, false) {
            Some(path) => {
                assert_eq!(path.dest(), next.path.dest(), "Path has the wrong end after splicing");
                let mut ret = PathExecutor::new(path);
                ret.path_position = self.path_position;
                ret.current_movement_original_cost_estimate = self.current_movement_original_cost_estimate;
                ret.cost_estimate_index = self.cost_estimate_index;
                ret.ticks_on_current = self.ticks_on_current;
                Some(ret)
            }
            None => self.cut_if_too_long(max_history, cutoff_amount),
        }
    }

    /// `cutIfTooLong`: drop the oldest part of a path once it is longer than `maxPathHistoryLength`.
    fn cut_if_too_long(&self, max_history: i32, cutoff_amount: i32) -> Option<PathExecutor> {
        if self.path_position as i32 > max_history {
            let cut = cutoff_amount.max(0) as usize;
            let new_path = self.path.cutoff(cut, self.path.len() - 1);
            assert_eq!(new_path.dest(), self.path.dest(), "Path has the wrong end after trimming its start");
            log::debug!(
                "[Presto] Discarding earliest segment movements, length cut from {} to {}",
                self.path.len(),
                new_path.len()
            );
            let mut ret = PathExecutor::new(new_path);
            ret.path_position = self.path_position - cut;
            ret.current_movement_original_cost_estimate = self.current_movement_original_cost_estimate;
            ret.cost_estimate_index = self.cost_estimate_index.map(|i| i.saturating_sub(cut));
            ret.ticks_on_current = self.ticks_on_current;
            return Some(ret);
        }
        None
    }
}

fn dir_of(path: &Path, i: usize) -> BlockPos {
    path.movements()[i].direction()
}

fn kind_name(path: &Path, i: usize) -> Option<&'static str> {
    path.movements().get(i).map(|m| m.kind.name())
}

/// `skipNow`: centred enough to sprint straight into the ascend.
fn skip_now(ex: &Exec, current: &super::movement::Movement) -> bool {
    let p = ex.ctx.player.position;
    let dir = current.direction();
    let off_target = (dir.x as f64 * (current.src.z as f64 + 0.5 - p.z)).abs()
        + (dir.z as f64 * (current.src.x as f64 + 0.5 - p.x)).abs();
    if off_target > 0.1 {
        return false;
    }
    let head_bonk = current.src.subtract(dir).above_n(2);
    if fully_passable(&ex.ctx.world, head_bonk.x, head_bonk.y, head_bonk.z) {
        return true;
    }
    // Wait 0.3 blocks.
    let flat_dist = (dir.x as f64 * (head_bonk.x as f64 + 0.5 - p.x)).abs()
        + (dir.z as f64 * (head_bonk.z as f64 + 0.5 - p.z)).abs();
    flat_dist > 0.8
}
