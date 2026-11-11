//! `MineProcess`: mine blocks (`#mine`): find them in the loaded chunks and the cache, path to the nearest,
//! break them, pick up what drops, and explore for more.
//!
//! Rescans run on the worker pool with every core scanning chunks, so the game thread never waits for one.

use super::{PathingCommand, PathingCommandType, Process, ProcessCtx};
use crate::behavior::input::Input;
use crate::bot::Bot;
use crate::cache::CachedWorld;
use crate::math::BlockPos;
use crate::pathing::context::CalculationContext;
use crate::pathing::costs::COST_INF;
use crate::pathing::goals::{Goal, GoalBlock, GoalComposite, GoalRef, GoalRunAway, GoalTwoBlocks};
use crate::pathing::helper;
use crate::player::EntityInfo;
use crate::settings::Settings;
use crate::utils::block_optional_meta::BlockOptionalMetaLookup;
use crate::utils::{scanner, worker};
use crate::world::flags;
use std::any::Any;
use std::fmt;
use std::sync::Arc;

/// `GoalThreeBlocks`: feet, head, or the block above the head at a position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GoalThreeBlocks(pub GoalTwoBlocks);

impl Goal for GoalThreeBlocks {
    fn is_in_goal(&self, x: i32, y: i32, z: i32) -> bool {
        let g = &self.0;
        x == g.x && (y == g.y || y == g.y - 1 || y == g.y - 2) && z == g.z
    }
    fn heuristic(&self, x: i32, y: i32, z: i32) -> f64 {
        let g = &self.0;
        let y_diff = y - g.y;
        GoalBlock::calculate(
            (x - g.x) as f64,
            if y_diff < -1 {
                y_diff + 2
            } else if y_diff == -1 {
                0
            } else {
                y_diff
            },
            (z - g.z) as f64,
        )
    }
    fn render_pos(&self) -> Option<BlockPos> {
        Some(BlockPos::new(self.0.x, self.0.y, self.0.z))
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn same_as(&self, other: &dyn Goal) -> bool {
        other.as_any().downcast_ref::<Self>().is_some_and(|o| o == self)
    }
}

impl fmt::Display for GoalThreeBlocks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let g = &self.0;
        write!(f, "GoalThreeBlocks{{x={},y={},z={}}}", g.x, g.y, g.z)
    }
}

/// `GoalRunAway` that is never reached (`isInGoal` false, `heuristic()` minus infinity): keep exploring.
#[derive(Clone, Debug, PartialEq)]
pub struct GoalExplore(pub GoalRunAway);

impl Goal for GoalExplore {
    fn is_in_goal(&self, _x: i32, _y: i32, _z: i32) -> bool {
        false
    }
    fn heuristic(&self, x: i32, y: i32, z: i32) -> f64 {
        self.0.heuristic(x, y, z)
    }
    fn heuristic_inside(&self) -> f64 {
        f64::NEG_INFINITY
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn same_as(&self, other: &dyn Goal) -> bool {
        other.as_any().downcast_ref::<Self>().is_some_and(|o| o == self)
    }
}

impl fmt::Display for GoalExplore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// `plausibleToBreak`: breakable, not something to avoid, and not sealed between bedrock.
pub fn plausible_to_break(ctx: &CalculationContext, pos: BlockPos) -> bool {
    let state = ctx.get_pos(pos);
    if helper::get_mining_duration_ticks_state(ctx, pos.x, pos.y, pos.z, state, true) >= COST_INF {
        return false;
    }
    if helper::avoid_breaking(ctx, pos.x, pos.y, pos.z, ctx.table.get(state)) {
        return false;
    }
    let bedrock = |p: BlockPos| &*ctx.info_pos(p).name == "minecraft:bedrock";
    !(bedrock(pos.above()) && bedrock(pos.below()))
}

/// `isNextToAir`: within `allowOnlyExposedOresDistance` (Manhattan) of air or a liquid block.
pub fn is_next_to_air(ctx: &CalculationContext, pos: BlockPos, radius: i32) -> bool {
    for dx in -radius..=radius {
        for dy in -radius..=radius {
            for dz in -radius..=radius {
                if dx.abs() + dy.abs() + dz.abs() <= radius
                    && helper::is_transparent(ctx.info(pos.x + dx, pos.y + dy, pos.z + dz))
                {
                    return true;
                }
            }
        }
    }
    false
}

/// `prune`: keep what is still there (or unloaded), plausible, allowed by the settings, not blacklisted;
/// nearest first, at most `max`. Drops near a still-minable block are dropped as goals.
pub fn prune(
    ctx: &CalculationContext,
    locs: Vec<BlockPos>,
    filter: &BlockOptionalMetaLookup,
    max: usize,
    blacklist: &[BlockPos],
    dropped: &mut Vec<BlockPos>,
    player_block: BlockPos,
) -> Vec<BlockPos> {
    let s = &ctx.settings;
    dropped.retain(|drop| {
        !locs.iter().any(|pos| {
            pos.dist_sqr(*drop) <= 9.0 && filter.has(ctx.get_pos(*pos)) && plausible_to_break(ctx, *pos)
        })
    });
    let mut seen = rustc_hash::FxHashSet::default();
    let min_y = s.min_y_level_while_mining + ctx.min_y;
    let mut out: Vec<BlockPos> = locs
        .into_iter()
        .filter(|p| seen.insert(*p))
        // Within loaded chunks, it must still be what we want (or a drop).
        .filter(|p| !ctx.bsi.world_contains_loaded_chunk(p.x, p.z) || filter.has(ctx.get_pos(*p)) || dropped.contains(p))
        // Encased in bedrock, or touching lava: implausible.
        .filter(|p| plausible_to_break(ctx, *p))
        .filter(|p| !s.allow_only_exposed_ores || is_next_to_air(ctx, *p, s.allow_only_exposed_ores_distance))
        .filter(|p| p.y >= min_y)
        .filter(|p| p.y <= s.max_y_level_while_mining)
        .filter(|p| !blacklist.contains(p))
        .collect();
    out.sort_by(|a, b| player_block.dist_sqr(*a).total_cmp(&player_block.dist_sqr(*b)));
    out.truncate(max);
    out
}

/// `searchWorld`: tracked blocks from the cache within two regions, then the loaded chunks, plus what we
/// already knew; pruned.
#[allow(clippy::too_many_arguments)]
pub fn search_world(
    ctx: &CalculationContext,
    cache: Option<&CachedWorld>,
    filter: &BlockOptionalMetaLookup,
    max: usize,
    already_known: &[BlockPos],
    blacklist: &[BlockPos],
    dropped: &mut Vec<BlockPos>,
    feet: BlockPos,
    player_block: BlockPos,
) -> Vec<BlockPos> {
    let s = ctx.settings.clone();
    let mut locs = Vec::new();
    let mut untracked = false;
    for bom in filter.blocks() {
        if crate::cache::chunk::keeps_track_of(&bom.name)
            && let Some(cache) = cache
        {
            // A region distance squared of 2: adjacent, directly or diagonally.
            locs.extend(cache.locations_of(&bom.name, s.max_cached_world_scan_count.max(0) as usize, feet.x, feet.z, 2));
        } else {
            untracked = true;
        }
    }
    locs = prune(ctx, locs, filter, max, blacklist, dropped, player_block);
    if untracked || (s.extend_cache_on_threshold && locs.len() < max) {
        let view = ctx.bsi.view();
        locs.extend(scanner::scan_chunk_radius(
            &*view.source,
            &view.dimension,
            filter.states(),
            feet,
            max as i32,
            32,
        ));
    }
    locs.extend_from_slice(already_known);
    prune(ctx, locs, filter, max, blacklist, dropped, player_block)
}

/// What a background rescan found.
struct Rescan {
    locations: Vec<BlockPos>,
    /// Nothing known, and not exploring: give up.
    cancel: bool,
}

#[derive(Default)]
pub struct MineProcess {
    filter: Option<BlockOptionalMetaLookup>,
    known_ore_locations: Vec<BlockPos>,
    blacklist: Vec<BlockPos>,
    /// Where an ore was just mined, until when its drop may still appear (`anticipatedDrops`).
    anticipated_drops: Vec<(BlockPos, i64)>,
    branch_point: Option<BlockPos>,
    branch_point_runaway: Option<GoalRef>,
    desired_quantity: i32,
    tick_count: i32,
    rescan: Option<worker::Slot<Rescan>>,
    rescanning: bool,
}

impl MineProcess {
    /// `mine(quantity, filter)`: `None` stops.
    pub fn mine(&mut self, c: &mut ProcessCtx, quantity: i32, filter: Option<BlockOptionalMetaLookup>) {
        self.filter = filter;
        if self.filter_filter(c.bot).is_none() {
            self.filter = None;
        }
        self.desired_quantity = quantity;
        self.known_ore_locations.clear();
        self.blacklist.clear();
        self.branch_point = None;
        self.branch_point_runaway = None;
        self.anticipated_drops.clear();
        self.rescan = None;
        self.rescanning = false;
        if self.filter.is_some() {
            // The first scan happens here, on this thread, so mining starts right away.
            self.rescan_now(c);
        }
    }

    pub fn filter(&self) -> Option<&BlockOptionalMetaLookup> {
        self.filter.as_ref()
    }

    /// `cancel`.
    pub fn cancel(&mut self, bot: &mut Bot) {
        self.on_lost_control(bot);
    }

    /// `filterFilter`: with `allowBreak` off, only what `allowBreakAnyway` lists.
    fn filter_filter(&self, bot: &mut Bot) -> Option<BlockOptionalMetaLookup> {
        let filter = self.filter.as_ref()?;
        if !bot.settings.allow_break {
            let allowed: Vec<_> = filter
                .blocks()
                .iter()
                .filter(|b| bot.settings.allow_break_anyway.contains(&b.name))
                .cloned()
                .collect();
            if allowed.is_empty() {
                bot.log_direct("Unable to mine when allowBreak is false and target block is not in allowBreakAnyway!");
                return None;
            }
            return Some(BlockOptionalMetaLookup::new(allowed));
        }
        Some(filter.clone())
    }

    /// `droppedItemsScan`: dropped items we want, and where drops are expected.
    fn dropped_items_scan(&self, settings: &Settings, entities: &[EntityInfo], filter: &BlockOptionalMetaLookup) -> Vec<BlockPos> {
        if !settings.mine_scan_dropped_items {
            return Vec::new();
        }
        let mut ret: Vec<BlockPos> = entities
            .iter()
            .filter(|e| e.item.as_deref().is_some_and(|i| filter.has_item(i)))
            .map(|e| BlockPos::of_vec(e.position))
            .collect();
        ret.extend(self.anticipated_drops.iter().map(|(p, _)| *p));
        ret
    }

    fn rescan_now(&mut self, c: &mut ProcessCtx) {
        let Some(filter) = self.filter_filter(c.bot) else { return };
        if c.bot.settings.legit_mine {
            return;
        }
        let Some(ctx) = CalculationContext::new(c.pc.game, &c.bot.settings, false, false) else {
            return;
        };
        let mut dropped = self.dropped_items_scan(&c.bot.settings, &c.pc.game.entities, &filter);
        let cache = c.bot.world.current().map(|w| w.cache.clone());
        let feet = c.pc.player_feet();
        let player_block = BlockPos::of_vec(c.pc.player.position);
        let max = c.bot.settings.mine_max_ore_locations_count.max(0) as usize;
        let mut locs = search_world(&ctx, cache.as_deref(), &filter, max, &[], &self.blacklist, &mut dropped, feet, player_block);
        locs.extend(dropped);
        if locs.is_empty() && !c.bot.settings.explore_for_blocks {
            c.bot.log_direct(&format!("No locations for {filter} known, cancelling"));
            if c.bot.settings.notification_on_mine_fail {
                c.bot.log_notification(&format!("No locations for {filter} known, cancelling"), true);
            }
            self.on_lost_control(c.bot);
            return;
        }
        self.known_ore_locations = locs;
    }

    /// Starts a rescan on the worker pool (`rescan` on `Baritone.getExecutor()`).
    fn rescan_in_background(&mut self, c: &mut ProcessCtx, filter: BlockOptionalMetaLookup) {
        if self.rescanning || c.bot.settings.legit_mine {
            return;
        }
        let Some(ctx) = CalculationContext::new(c.pc.game, &c.bot.settings, false, true) else {
            return;
        };
        let mut dropped = self.dropped_items_scan(&c.bot.settings, &c.pc.game.entities, &filter);
        let cache = c.bot.world.current().map(|w| w.cache.clone());
        let feet = c.pc.player_feet();
        let player_block = BlockPos::of_vec(c.pc.player.position);
        let already = self.known_ore_locations.clone();
        let blacklist = self.blacklist.clone();
        let max = c.bot.settings.mine_max_ore_locations_count.max(0) as usize;
        let explore = c.bot.settings.explore_for_blocks;
        let slot = self.rescan.get_or_insert_with(worker::Slot::new).clone();
        self.rescanning = true;
        worker::execute(move || {
            let mut locs = search_world(&ctx, cache.as_deref(), &filter, max, &already, &blacklist, &mut dropped, feet, player_block);
            locs.extend(dropped);
            let cancel = locs.is_empty() && !explore;
            slot.put(Rescan { locations: locs, cancel });
        });
    }

    fn take_rescan(&mut self, bot: &mut Bot) {
        let Some(slot) = &self.rescan else { return };
        let Some(r) = slot.take() else { return };
        self.rescanning = false;
        if r.cancel {
            let filter = self.filter.as_ref().map(|f| f.to_string()).unwrap_or_default();
            bot.log_direct(&format!("No locations for {filter} known, cancelling"));
            if bot.settings.notification_on_mine_fail {
                bot.log_notification(&format!("No locations for {filter} known, cancelling"), true);
            }
            self.on_lost_control(bot);
            return;
        }
        self.known_ore_locations = r.locations;
    }

    /// `updateLoucaSystem`: remember where an ore under the crosshair is, for its drop.
    fn update_louca_system(&mut self, c: &ProcessCtx) {
        let now = c.pc.game.now_ms;
        if let Some(pos) = c.pc.selected_block()
            && self.known_ore_locations.contains(&pos)
        {
            let until = now + c.bot.settings.mine_drop_loiter_duration_ms_thanks_louca;
            match self.anticipated_drops.iter_mut().find(|(p, _)| *p == pos) {
                Some(e) => e.1 = until,
                None => self.anticipated_drops.push((pos, until)),
            }
        }
        self.anticipated_drops.retain(|(_, until)| *until >= now);
    }

    /// `internalMiningGoal`.
    fn internal_mining_goal(&self, pos: BlockPos, ctx: &CalculationContext, locs: &[BlockPos], filter: &BlockOptionalMetaLookup) -> bool {
        if locs.contains(&pos) {
            return true;
        }
        let state = ctx.get_pos(pos);
        if ctx.settings.internal_mining_air_exception && ctx.table.get(state).is_air() {
            return true;
        }
        filter.has(state) && plausible_to_break(ctx, pos)
    }

    /// `coalesce`: the goal for one ore, joined with what's above and below it.
    fn coalesce(&self, loc: BlockPos, locs: &[BlockPos], ctx: &CalculationContext, filter: &BlockOptionalMetaLookup) -> GoalRef {
        let assume_vertical_shaft_mine = !ctx.info_pos(loc.above()).is(flags::FALLING_BLOCK);
        if !ctx.settings.force_internal_mining {
            return if assume_vertical_shaft_mine {
                // We can get directly below it.
                Arc::new(GoalThreeBlocks(GoalTwoBlocks::at(loc)))
            } else {
                // Feet or head into it.
                Arc::new(GoalTwoBlocks::at(loc))
            };
        }
        let upward = self.internal_mining_goal(loc.above(), ctx, locs, filter);
        let downward = self.internal_mining_goal(loc.below(), ctx, locs, filter);
        let double_downward = self.internal_mining_goal(loc.below_n(2), ctx, locs, filter);
        if upward == downward {
            return if double_downward && assume_vertical_shaft_mine {
                // A checkerboard: this one and the one two below; standing just below covers both.
                Arc::new(GoalThreeBlocks(GoalTwoBlocks::at(loc)))
            } else {
                Arc::new(GoalTwoBlocks::at(loc))
            };
        }
        if upward {
            // Feet in this one and head in the one above.
            return Arc::new(GoalBlock::at(loc));
        }
        if double_downward && assume_vertical_shaft_mine {
            return Arc::new(GoalTwoBlocks::at(loc.below()));
        }
        Arc::new(GoalBlock::at(loc.below()))
    }

    /// `addNearby` (legit mining): only what can be seen from here, as a player would.
    fn add_nearby(&mut self, c: &mut ProcessCtx) -> bool {
        let Some(filter) = self.filter_filter(c.bot) else {
            return false;
        };
        let mut dropped = self.dropped_items_scan(&c.bot.settings, &c.pc.game.entities, &filter);
        self.known_ore_locations.extend(dropped.iter().copied());
        let feet = c.pc.player_feet();
        let world = &c.pc.world;
        let search_dist = 10;
        let faked_reach = 20.0;
        let diagonals = c.bot.settings.legit_mine_include_diagonals;
        for x in feet.x - search_dist..=feet.x + search_dist {
            for y in feet.y - search_dist..=feet.y + search_dist {
                for z in feet.z - search_dist..=feet.z + search_dist {
                    if !filter.has(world.get(x, y, z)) {
                        continue;
                    }
                    let pos = BlockPos::new(x, y, z);
                    // Only what we can see: otherwise this is an x-ray.
                    if (diagonals && self.known_ore_locations.iter().any(|o| o.dist_sqr(pos) <= 2.0))
                        || c.pc.reachable(c.bot.look.aim(), pos, faked_reach, false).is_some()
                    {
                        self.known_ore_locations.push(pos);
                    }
                }
            }
        }
        let max = c.bot.settings.mine_max_ore_locations_count.max(0) as usize;
        let locs = std::mem::take(&mut self.known_ore_locations);
        self.known_ore_locations = prune(world, locs, &filter, max, &self.blacklist, &mut dropped, BlockPos::of_vec(c.pc.player.position));
        true
    }

    /// `updateGoal`.
    fn update_goal(&mut self, c: &mut ProcessCtx) -> Option<PathingCommand> {
        let filter = self.filter_filter(c.bot)?;
        let legit = c.bot.settings.legit_mine;
        if !self.known_ore_locations.is_empty() {
            let world = &c.pc.world;
            let mut dropped = self.dropped_items_scan(&c.bot.settings, &c.pc.game.entities, &filter);
            let max = c.bot.settings.mine_max_ore_locations_count.max(0) as usize;
            let locs = prune(
                world,
                self.known_ore_locations.clone(),
                &filter,
                max,
                &self.blacklist,
                &mut dropped,
                BlockPos::of_vec(c.pc.player.position),
            );
            let goals: Vec<GoalRef> = locs.iter().map(|l| self.coalesce(*l, &locs, world, &filter)).collect();
            self.known_ore_locations = locs;
            return Some(PathingCommand::new(
                Some(Arc::new(GoalComposite::new(goals))),
                if legit {
                    PathingCommandType::ForceRevalidateGoalAndPath
                } else {
                    PathingCommandType::RevalidateGoalAndPath
                },
            ));
        }
        // No known ores: explore, if allowed.
        if !legit && !c.bot.settings.explore_for_blocks {
            return None;
        }
        let y = c.bot.settings.legit_mine_y_level;
        let branch = *self.branch_point.get_or_insert_with(|| c.pc.player_feet());
        let goal = self
            .branch_point_runaway
            .get_or_insert_with(|| Arc::new(GoalExplore(GoalRunAway::new(1.0, Some(y), vec![branch]))))
            .clone();
        Some(PathingCommand::new(Some(goal), PathingCommandType::RevalidateGoalAndPath))
    }
}

impl Process for MineProcess {
    fn is_active(&mut self, _c: &mut ProcessCtx) -> bool {
        self.filter.is_some()
    }

    fn on_tick(&mut self, c: &mut ProcessCtx, calc_failed: bool, is_safe_to_cancel: bool) -> Option<PathingCommand> {
        self.take_rescan(c.bot);
        let filter = self.filter.clone()?;
        if self.desired_quantity > 0 {
            let curr: i32 = c
                .pc
                .inventory()
                .items
                .iter()
                .filter(|s| !s.is_empty() && filter.has_item(&s.id))
                .map(|s| s.count)
                .sum();
            if curr >= self.desired_quantity {
                c.bot.log_direct(&format!("Have {curr} valid items"));
                self.cancel(c.bot);
                return None;
            }
        }
        if calc_failed {
            if !self.known_ore_locations.is_empty() && c.bot.settings.blacklist_closest_on_failure {
                let msg = format!("Unable to find any path to {filter}, blacklisting presumably unreachable closest instance...");
                c.bot.log_direct(&msg);
                if c.bot.settings.notification_on_mine_fail {
                    c.bot.log_notification(&msg, true);
                }
                let feet = c.pc.player_feet();
                if let Some(closest) = self
                    .known_ore_locations
                    .iter()
                    .copied()
                    .min_by(|a, b| feet.dist_sqr(*a).total_cmp(&feet.dist_sqr(*b)))
                {
                    self.blacklist.push(closest);
                }
                let bl = self.blacklist.clone();
                self.known_ore_locations.retain(|p| !bl.contains(p));
            } else {
                let msg = format!("Unable to find any path to {filter}, canceling mine");
                c.bot.log_direct(&msg);
                if c.bot.settings.notification_on_mine_fail {
                    c.bot.log_notification(&msg, true);
                }
                self.cancel(c.bot);
                return None;
            }
        }
        self.update_louca_system(c);
        let interval = c.bot.settings.mine_goal_update_interval;
        let tc = self.tick_count;
        self.tick_count += 1;
        if interval != 0 && tc % interval == 0 {
            self.rescan_in_background(c, filter.clone());
        }
        // What was known before `addNearby` (`curr`).
        let curr = self.known_ore_locations.clone();
        if c.bot.settings.legit_mine && !self.add_nearby(c) {
            self.cancel(c.bot);
            return None;
        }
        // A block of ours straight above: mine it from here without moving (shaft mining).
        let feet = c.pc.player_feet();
        let world = &c.pc.world;
        let shaft = curr
            .iter()
            .copied()
            .filter(|p| p.x == feet.x && p.z == feet.z && p.y >= feet.y)
            // After a block breaks, the list takes up to mineGoalUpdateInterval ticks to update.
            .filter(|p| !world.info_pos(*p).is_air())
            .min_by(|a, b| feet.above().dist_sqr(*a).total_cmp(&feet.above().dist_sqr(*b)));
        c.bot.input.clear_all_keys();
        if let Some(pos) = shaft
            && c.pc.player.on_ground
        {
            let info = world.info_pos(pos);
            if !helper::avoid_breaking(world, pos.x, pos.y, pos.z, info)
                && let Some(rot) = c.pc.reachable(c.bot.look.aim(), pos, c.pc.block_reach_distance(), false)
                && is_safe_to_cancel
            {
                c.bot
                    .look
                    .update_target(rot, true, &c.bot.settings, c.pc.player.fall_flying);
                let s = &c.bot.settings;
                if s.auto_tool && !s.assume_external_auto_tool {
                    let slot = world.tool_set.best_slot(info, s.prefer_silk_touch, false);
                    c.bot.inventory.select(slot);
                }
                // With instantAim the turn and the hit happen in this one tick.
                if s.instant_aim || c.pc.is_looking_at(pos) || c.pc.player_rotations().is_really_close_to(rot) {
                    c.bot.input.set_input_force_state(Input::ClickLeft, true);
                }
                return Some(PathingCommand::new(None, PathingCommandType::RequestPause));
            }
        }
        match self.update_goal(c) {
            Some(cmd) => Some(cmd),
            None => {
                // None in range.
                self.cancel(c.bot);
                None
            }
        }
    }

    fn on_lost_control(&mut self, _bot: &mut Bot) {
        self.filter = None;
        self.desired_quantity = 0;
        self.known_ore_locations.clear();
        self.blacklist.clear();
        self.branch_point = None;
        self.branch_point_runaway = None;
        self.anticipated_drops.clear();
        self.rescan = None;
        self.rescanning = false;
    }

    fn display_name(&self) -> String {
        match &self.filter {
            Some(f) => format!("Mine {f}"),
            None => "Mine null".into(),
        }
    }
}
