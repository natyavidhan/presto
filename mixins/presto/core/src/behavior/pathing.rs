//! `PathingBehavior`: the goal, the path being walked, the next one planned, and the searches that find
//! them. Searches run on threads of their own; the game thread picks their results up at the start of a tick
//! and never waits on one.

use crate::ctx::PlayerContext;
use crate::math::BlockPos;
use crate::pathing::calc::{AStarPathFinder, Favoring, PathCalculationResult, ResultType, SearchLimits, SearchShared};
use crate::pathing::context::{CalculationContext, Protection};
use crate::pathing::executor::PathExecutor;
use crate::pathing::goals::{GoalRef, GoalXZ};
use crate::pathing::helper;
use crate::pathing::movement::Exec;
use std::sync::mpsc;
use std::sync::Arc;

/// `PathEvent`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathEvent {
    CalcStarted,
    CalcFinishedNowExecuting,
    CalcFailed,
    NextSegmentCalcStarted,
    NextSegmentCalcFinished,
    ContinuingOntoPlannedNext,
    SplicingOntoNextEarly,
    AtGoal,
    PathFinishedNextStillCalculating,
    NextCalcFailed,
    DiscardNext,
    Canceled,
}

/// A search running on another thread.
pub struct InProgress {
    pub shared: Arc<SearchShared>,
    pub start: BlockPos,
    pub goal: GoalRef,
    rx: mpsc::Receiver<PathCalculationResult>,
    talk_about_it: bool,
}

impl InProgress {
    /// `bestPathSoFar`'s positions.
    pub fn best_so_far(&self) -> Option<Arc<Vec<BlockPos>>> {
        self.shared.progress().best_so_far
    }
    pub fn most_recent(&self) -> Option<Arc<Vec<BlockPos>>> {
        self.shared.progress().most_recent
    }
}

#[derive(Default)]
pub struct PathingBehavior {
    current: Option<PathExecutor>,
    next: Option<PathExecutor>,
    goal: Option<GoalRef>,
    /// The cost overrides of the process that set the goal (`PathingCommandContext`).
    protection: Option<Arc<dyn Protection>>,
    ticks_elapsed_so_far: i32,
    start_position: Option<BlockPos>,
    safe_to_cancel: bool,
    pause_requested_last_tick: bool,
    unpaused_last_tick: bool,
    paused_this_tick: bool,
    cancel_requested: bool,
    calc_failed_last_tick: bool,
    in_progress: Option<InProgress>,
    expected_segment_start: BlockPos,
    to_dispatch: Vec<PathEvent>,
    /// Debug lines from the search threads' results, printed on the game thread.
    pub debug: Vec<String>,
}

impl PathingBehavior {
    fn queue(&mut self, e: PathEvent) {
        self.to_dispatch.push(e);
    }

    /// `dispatchEvents`: the events since last time, for the event listeners.
    pub fn dispatch_events(&mut self) -> Vec<PathEvent> {
        let events = std::mem::take(&mut self.to_dispatch);
        self.calc_failed_last_tick = events.contains(&PathEvent::CalcFailed);
        events
    }

    pub fn goal(&self) -> Option<&GoalRef> {
        self.goal.as_ref()
    }
    pub fn current(&self) -> Option<&PathExecutor> {
        self.current.as_ref()
    }
    pub fn next(&self) -> Option<&PathExecutor> {
        self.next.as_ref()
    }
    pub fn in_progress(&self) -> Option<&InProgress> {
        self.in_progress.as_ref()
    }
    pub fn has_path(&self) -> bool {
        self.current.is_some()
    }
    /// `isPathing`.
    pub fn is_pathing(&self) -> bool {
        self.has_path() && !self.paused_this_tick
    }
    pub fn calc_failed_last_tick(&self) -> bool {
        self.calc_failed_last_tick
    }
    pub fn expected_segment_start(&self) -> BlockPos {
        self.expected_segment_start
    }
    pub fn set_expected_segment_start(&mut self, p: BlockPos) {
        self.expected_segment_start = p;
    }
    pub fn tick_elapsed(&mut self) {
        self.ticks_elapsed_so_far += 1;
    }
    pub fn is_sprinting(&self) -> Option<bool> {
        self.is_pathing().then(|| self.current.as_ref().is_some_and(|c| c.is_sprinting()))
    }

    /// `isSafeToCancel` (the elytra process's say is folded in by the caller).
    pub fn is_safe_to_cancel(&self) -> bool {
        if self.current.is_none() {
            return true;
        }
        self.safe_to_cancel
    }

    pub fn request_pause(&mut self) {
        self.pause_requested_last_tick = true;
    }

    /// `cancelSegmentIfSafe`.
    pub fn cancel_segment_if_safe(&mut self, ex_input: &mut crate::behavior::input::InputOverrideHandler) -> bool {
        if self.is_safe_to_cancel() {
            self.secret_internal_segment_cancel(ex_input);
            return true;
        }
        false
    }

    /// `softCancelIfSafe`: drop the paths but leave the keys (the next path takes over without a stutter).
    pub fn soft_cancel_if_safe(&mut self) {
        if let Some(p) = &self.in_progress {
            p.shared.cancel();
        }
        if !self.is_safe_to_cancel() {
            return;
        }
        self.current = None;
        self.next = None;
        self.cancel_requested = true;
    }

    /// `secretInternalSegmentCancel`: cancel the current path.
    pub fn secret_internal_segment_cancel(&mut self, input: &mut crate::behavior::input::InputOverrideHandler) {
        self.queue(PathEvent::Canceled);
        if let Some(p) = &self.in_progress {
            p.shared.cancel();
        }
        if self.current.is_some() {
            self.current = None;
            self.next = None;
            input.clear_all_keys();
            input.break_helper.stop_breaking_block();
        }
    }

    /// `forceCancel`.
    pub fn force_cancel(&mut self, input: &mut crate::behavior::input::InputOverrideHandler) {
        self.secret_internal_segment_cancel(input);
        self.in_progress = None;
    }

    pub fn secret_internal_set_goal(&mut self, goal: Option<GoalRef>) {
        self.goal = goal;
    }

    /// `secretInternalSetGoalAndPath`: set the goal, and start a search if nothing is running.
    pub fn secret_internal_set_goal_and_path(
        &mut self,
        goal: Option<GoalRef>,
        protection: Option<Arc<dyn Protection>>,
        ctx: &PlayerContext,
        has_throwaway: bool,
    ) -> bool {
        self.secret_internal_set_goal(goal);
        self.protection = protection;
        let Some(goal) = self.goal.clone() else {
            return false;
        };
        if goal.is_in_goal_pos(ctx.player_feet()) {
            return false;
        }
        if self.current.is_some() {
            return false;
        }
        if self.in_progress.is_some() {
            return false;
        }
        self.queue(PathEvent::CalcStarted);
        let start = self.expected_segment_start;
        self.find_path_in_new_thread(start, true, ctx, has_throwaway);
        true
    }

    /// Takes in the result of a finished search, as Java's search thread does when it ends.
    pub fn poll_search(&mut self) {
        let Some(p) = &self.in_progress else { return };
        let result = match p.rx.try_recv() {
            Ok(r) => r,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => PathCalculationResult {
                kind: ResultType::Exception,
                path: None,
            },
        };
        let p = self.in_progress.take().expect("checked above");
        let start = p.start;
        let goal = p.goal.clone();
        let executor = result.path.map(PathExecutor::new);
        match (&self.current, executor) {
            (None, Some(ex)) => {
                if ex.path().contains(self.expected_segment_start) {
                    self.queue(PathEvent::CalcFinishedNowExecuting);
                    self.current = Some(ex);
                    self.reset_estimated_ticks_to_goal(start);
                } else {
                    self.debug.push("Warning: discarding orphan path segment with incorrect start".into());
                }
            }
            (None, None) => {
                if result.kind != ResultType::Cancellation && result.kind != ResultType::Exception {
                    self.queue(PathEvent::CalcFailed);
                }
            }
            (Some(cur), ex) => {
                if self.next.is_none() {
                    match ex {
                        Some(ex) if ex.path().src() == cur.path().dest() => {
                            self.queue(PathEvent::NextSegmentCalcFinished);
                            self.next = Some(ex);
                        }
                        Some(_) => self
                            .debug
                            .push("Warning: discarding orphan next segment with incorrect start".into()),
                        None => self.queue(PathEvent::NextCalcFailed),
                    }
                } else {
                    self.debug
                        .push("Warning: PathingBehavior illegal state! Discarding invalid path!".into());
                }
            }
        }
        if p.talk_about_it
            && let Some(cur) = &self.current
        {
            let nodes = cur.path().num_nodes;
            if goal.is_in_goal_pos(cur.path().dest()) {
                self.debug.push(format!(
                    "Finished finding a path from {start} to {goal}. {nodes} nodes considered"
                ));
            } else {
                self.debug.push(format!(
                    "Found path segment from {start} towards {goal}. {nodes} nodes considered"
                ));
            }
        }
    }

    /// `tickPath`.
    pub fn tick_path(&mut self, ex: &mut Exec, has_throwaway: bool) {
        self.paused_this_tick = false;
        if self.pause_requested_last_tick && self.safe_to_cancel {
            self.pause_requested_last_tick = false;
            if self.unpaused_last_tick {
                ex.input.clear_all_keys();
                ex.input.break_helper.stop_breaking_block();
            }
            self.unpaused_last_tick = false;
            self.paused_this_tick = true;
            return;
        }
        self.unpaused_last_tick = true;
        if self.cancel_requested {
            self.cancel_requested = false;
            ex.input.clear_all_keys();
        }
        let feet = ex.ctx.player_feet();
        let expected = self.expected_segment_start;
        if let Some(p) = &self.in_progress {
            // Are we calculating the right thing, though?
            let calc_from = p.start;
            let best = p.best_so_far();
            if self.current.as_ref().is_none_or(|c| c.path().dest() != calc_from)
                && calc_from != feet
                && calc_from != expected
                && best
                    .as_ref()
                    .is_none_or(|b| !b.contains(&feet) && !b.contains(&expected))
            {
                // Just started, best is empty, so calc_from is checked too.
                p.shared.cancel();
            }
        }
        if self.current.is_none() {
            return;
        }
        let best = self.in_progress.as_ref().and_then(|p| p.best_so_far());
        let safe = self
            .current
            .as_mut()
            .expect("checked")
            .on_tick(ex, best.as_deref().map(|v| v.as_slice()));
        self.safe_to_cancel = safe;
        let cur = self.current.as_ref().expect("checked");
        if cur.failed() || cur.finished() {
            self.current = None;
            let feet = ex.ctx.player_feet();
            if self.goal.as_ref().is_none_or(|g| g.is_in_goal_pos(feet)) {
                ex.log_debug(format!(
                    "All done. At {}",
                    self.goal.as_ref().map_or("no goal".to_string(), |g| g.to_string())
                ));
                self.queue(PathEvent::AtGoal);
                self.next = None;
                return;
            }
            if let Some(next) = &self.next
                && !next.path().contains(feet)
                && !next.path().contains(expected)
            {
                // The current path failed midway: we aren't on the planned next one.
                ex.log_debug("Discarding next path as it does not contain current position");
                self.queue(PathEvent::DiscardNext);
                self.next = None;
            }
            if let Some(next) = self.next.take() {
                ex.log_debug("Continuing on to planned next path");
                self.queue(PathEvent::ContinuingOntoPlannedNext);
                self.current = Some(next);
                let best = self.in_progress.as_ref().and_then(|p| p.best_so_far());
                // Get started right away rather than waste a tick.
                let _ = self
                    .current
                    .as_mut()
                    .expect("just set")
                    .on_tick(ex, best.as_deref().map(|v| v.as_slice()));
                return;
            }
            // Current just ended, we aren't in the goal, and there's no plan.
            if self.in_progress.is_some() {
                self.queue(PathEvent::PathFinishedNextStillCalculating);
                return;
            }
            self.queue(PathEvent::CalcStarted);
            self.find_path_in_new_thread(expected, true, ex.ctx, has_throwaway);
            return;
        }
        // The current path is in progress.
        if self.safe_to_cancel && self.next.is_some() {
            let snapped = self.next.as_mut().expect("checked").snipsnap_if_possible(ex);
            if snapped {
                // A movement just ended: hop onto the next path.
                ex.log_debug("Splicing into planned next path early...");
                self.queue(PathEvent::SplicingOntoNextEarly);
                self.current = self.next.take();
                let best = self.in_progress.as_ref().and_then(|p| p.best_so_far());
                let _ = self
                    .current
                    .as_mut()
                    .expect("just set")
                    .on_tick(ex, best.as_deref().map(|v| v.as_slice()));
                return;
            }
        }
        let settings = ex.ctx.settings;
        if settings.splice_path
            && let Some(spliced) = self.current.as_ref().expect("in progress").try_splice(
                self.next.as_ref(),
                settings.max_path_history_length,
                settings.path_history_cutoff_amount,
            )
        {
            self.current = Some(spliced);
        }
        if let (Some(c), Some(n)) = (&self.current, &self.next)
            && c.path().dest() == n.path().dest()
        {
            self.next = None;
        }
        if self.in_progress.is_some() || self.next.is_some() {
            return;
        }
        let cur = self.current.as_ref().expect("in progress");
        if self.goal.as_ref().is_none_or(|g| g.is_in_goal_pos(cur.path().dest())) {
            // This path gets us there.
            return;
        }
        if self.ticks_remaining_in_segment(false).is_some_and(|t| t < settings.planning_tick_lookahead as f64) {
            // 7.5 seconds or less left: plan the next segment. (Not counting the current movement, or a very
            // long last one would only start planning once it was done.)
            let dest = cur.path().dest();
            ex.log_debug("Path almost over. Planning ahead...");
            self.queue(PathEvent::NextSegmentCalcStarted);
            self.find_path_in_new_thread(dest, false, ex.ctx, has_throwaway);
        }
    }

    /// `ticksRemainingInSegment`: the cost of what's left of the current path.
    pub fn ticks_remaining_in_segment(&self, include_current: bool) -> Option<f64> {
        let cur = self.current.as_ref()?;
        let start = if include_current { cur.position() } else { cur.position() + 1 };
        Some(
            cur.path()
                .movements()
                .iter()
                .skip(start)
                .map(|m| m.cost())
                .sum(),
        )
    }

    /// `estimatedTicksToGoal`: from how fast the heuristic has fallen so far.
    pub fn estimated_ticks_to_goal(&mut self, feet: BlockPos) -> Option<f64> {
        let goal = self.goal.clone()?;
        let start = self.start_position?;
        if goal.is_in_goal_pos(feet) {
            self.reset_estimated_ticks_to_goal(self.expected_segment_start);
            return Some(0.0);
        }
        if self.ticks_elapsed_so_far == 0 {
            return None;
        }
        let current = goal.heuristic_pos(feet);
        let start_h = goal.heuristic_pos(start);
        if current == start_h {
            return None;
        }
        Some((current - goal.heuristic_inside()).abs() * self.ticks_elapsed_so_far as f64 / (start_h - current).abs())
    }

    fn reset_estimated_ticks_to_goal(&mut self, start: BlockPos) {
        self.ticks_elapsed_so_far = 0;
        self.start_position = Some(start);
    }

    /// `pathStart` (issue #209): where a new path starts: the feet, or the block the player is really
    /// standing over when hanging off an edge or mid-jump.
    pub fn path_start(ctx: &PlayerContext) -> BlockPos {
        let feet = ctx.player_feet();
        let world = &ctx.world;
        if !helper::can_walk_on_pos(world, feet.below()) {
            let p = ctx.player.position;
            if ctx.player.on_ground {
                let mut closest: Vec<BlockPos> = Vec::with_capacity(9);
                for dx in -1..=1 {
                    for dz in -1..=1 {
                        closest.push(BlockPos::new(feet.x + dx, feet.y, feet.z + dz));
                    }
                }
                let d = |b: &BlockPos| {
                    let x = b.x as f64 + 0.5 - p.x;
                    let z = b.z as f64 + 0.5 - p.z;
                    x * x + z * z
                };
                closest.sort_by(|a, b| d(a).total_cmp(&d(b)));
                for support in closest.iter().take(4) {
                    let x_dist = (support.x as f64 + 0.5 - p.x).abs();
                    let z_dist = (support.z as f64 + 0.5 - p.z).abs();
                    if x_dist > 0.8 && z_dist > 0.8 {
                        continue; // too far to be sneaking off this one
                    }
                    if helper::can_walk_on_pos(world, support.below())
                        && helper::can_walk_through_pos(world, *support)
                        && helper::can_walk_through_pos(world, support.above())
                    {
                        return *support;
                    }
                }
            } else if helper::can_walk_on_pos(world, feet.below_n(2)) {
                // Mid-jump.
                return feet.below();
            }
        }
        feet
    }

    /// `findPathInNewThread`.
    fn find_path_in_new_thread(&mut self, start: BlockPos, talk_about_it: bool, ctx: &PlayerContext, has_throwaway: bool) {
        assert!(self.in_progress.is_none(), "Already doing it");
        let Some(goal) = self.goal.clone() else {
            log::debug!("[Presto] no goal");
            return;
        };
        let s = ctx.settings;
        let (primary, failure) = if self.current.is_none() {
            (s.primary_timeout_ms, s.failure_timeout_ms)
        } else {
            (s.plan_ahead_primary_timeout_ms, s.plan_ahead_failure_timeout_ms)
        };
        let Some(mut calc) = CalculationContext::new(ctx.game, ctx.settings, has_throwaway, true) else {
            return;
        };
        calc.protection = self.protection.clone();
        // simplifyUnloadedYCoord: a goal in unloaded chunks becomes just its column.
        let mut transformed = goal.clone();
        if s.simplify_unloaded_y_coord
            && let Some(pos) = goal.render_pos()
            && !calc.bsi.world_contains_loaded_chunk(pos.x, pos.z)
        {
            log::debug!("[Presto] Simplifying {goal} to GoalXZ due to distance");
            transformed = Arc::new(GoalXZ::new(pos.x, pos.z));
        }
        let favoring = make_favoring(ctx, self.current.as_ref().map(|c| c.path().positions()), &calc);
        let feet = ctx.player_feet();
        let mut real_start = start;
        let sub = feet.subtract(real_start);
        if feet.y == real_start.y && sub.x.abs() <= 1 && sub.z.abs() <= 1 {
            real_start = feet;
        }
        let finder = AStarPathFinder::new(real_start, start, transformed, favoring, calc);
        let shared = finder.shared();
        let limits = SearchLimits {
            primary_timeout_ms: primary.max(0) as u64,
            failure_timeout_ms: failure.max(0) as u64,
            slow_path: s.slow_path,
            slow_path_delay_ms: s.slow_path_time_delay_ms.max(0) as u64,
            slow_path_timeout_ms: s.slow_path_timeout_ms.max(0) as u64,
            max_chunk_border_fetch: s.pathing_max_chunk_border_fetch,
            minimum_improvement_repropagation: s.minimum_improvement_repropagation,
            map_default_size: s.pathing_map_default_size.max(0) as usize,
        };
        if talk_about_it {
            log::debug!("[Presto] Starting to search for path from {start} to {goal}");
        }
        let (tx, rx) = mpsc::channel();
        // Try it right here first: a short path (the next ore, around a corner) is then walked this very
        // tick, not after a handoff to the search thread. A search that needs longer carries on on a
        // thread from where it got to.
        let budget = s.sync_pathing_budget_ms;
        let job: Box<dyn FnOnce() -> crate::pathing::calc::PathCalculationResult + Send> = if budget > 0 {
            match finder.calculate_within(limits, std::time::Duration::from_millis(budget as u64)) {
                Ok(result) => {
                    let _ = tx.send(result);
                    self.in_progress = Some(InProgress {
                        shared,
                        start,
                        goal,
                        rx,
                        talk_about_it,
                    });
                    self.poll_search();
                    return;
                }
                Err(unfinished) => Box::new(move || unfinished.resume()),
            }
        } else {
            Box::new(move || finder.calculate(limits))
        };
        let spawned = std::thread::Builder::new()
            .name("Presto path search".into())
            .spawn(move || {
                let _ = tx.send(job());
            });
        if spawned.is_err() {
            log::error!("[Presto] couldn't start a path search thread");
            return;
        }
        self.in_progress = Some(InProgress {
            shared,
            start,
            goal,
            rx,
            talk_about_it,
        });
    }
}

/// `new Favoring(ctx, previous, context)`: the previous path's positions, and `Avoidance` of mobs and spawners.
fn make_favoring(ctx: &PlayerContext, previous: Option<&[BlockPos]>, calc: &CalculationContext) -> Favoring {
    let s = ctx.settings;
    let mut f = Favoring::new(previous, calc.backtrack_cost_favoring_coefficient);
    if !s.avoidance {
        return f;
    }
    let feet = ctx.player_feet();
    let spawner_coeff = s.mob_spawner_avoidance_coefficient;
    let mob_coeff = s.mob_avoidance_coefficient;
    if spawner_coeff != 1.0
        && let Some(cache) = ctx.world.bsi.view().cache
    {
        for spawner in cache.locations_of("minecraft:spawner", 1, feet.x, feet.z, 2) {
            f.apply_spherical(spawner, spawner_coeff, s.mob_spawner_avoidance_radius);
        }
    }
    if mob_coeff != 1.0 {
        for e in &ctx.game.entities {
            // `entity instanceof Mob`: every mob, animals included.
            if !e.is_mob || !e.alive {
                continue;
            }
            // Spiders only in the dark; zombified piglins only once angered; endermen only when creepy.
            if &*e.kind == "minecraft:spider" && ctx.game.light_level >= 0.5 {
                continue;
            }
            if (&*e.kind == "minecraft:zombified_piglin" || &*e.kind == "minecraft:enderman") && !e.angry {
                continue;
            }
            f.apply_spherical(BlockPos::of_vec(e.position), mob_coeff, s.mob_avoidance_radius);
        }
    }
    f
}
