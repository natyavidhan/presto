//! `GetToBlockProcess`: go to the nearest block of a kind (`#goto <block>`), and open it on arrival when it is
//! a container.
//!
//! Rescans run on the worker pool; with `instantAim` the container is clicked in the tick the player turns to it.

use super::mine::{GoalExplore, search_world};
use super::{PathingCommand, PathingCommandType, Process, ProcessCtx};
use crate::behavior::input::Input;
use crate::behavior::pathing::PathingBehavior;
use crate::bot::Bot;
use crate::math::BlockPos;
use crate::pathing::context::CalculationContext;
use crate::pathing::goals::{GoalBlock, GoalComposite, GoalGetToBlock, GoalRunAway, GoalTwoBlocks};
use crate::pathing::{goals::GoalRef, helper};
use crate::utils::block_optional_meta::{BlockOptionalMeta, BlockOptionalMetaLookup};
use crate::utils::worker;
use std::sync::Arc;

/// How many locations to keep.
const MAX_LOCATIONS: usize = 64;

#[derive(Default)]
pub struct GetToBlockProcess {
    getting_to: Option<BlockOptionalMeta>,
    /// `None` until the first scan.
    known_locations: Option<Vec<BlockPos>>,
    /// Locations we failed to find a path to.
    blacklist: Vec<BlockPos>,
    start: Option<BlockPos>,
    tick_count: i32,
    arrival_tick_count: i32,
    rescan: Option<worker::Slot<Vec<BlockPos>>>,
    rescanning: bool,
}

impl GetToBlockProcess {
    /// `getToBlock`.
    pub fn get_to_block(&mut self, c: &mut ProcessCtx, block: BlockOptionalMeta) {
        self.on_lost_control(c.bot);
        self.getting_to = Some(block);
        self.start = Some(c.pc.player_feet());
        self.blacklist.clear();
        self.arrival_tick_count = 0;
        self.rescan_now(c, &[]);
    }

    pub fn getting_to(&self) -> Option<&BlockOptionalMeta> {
        self.getting_to.as_ref()
    }

    /// `GetToBlockCalculationContext`: tells the search that `allowBreak` doesn't matter here. It is never
    /// used to calculate a path.
    fn context(c: &ProcessCtx, threaded: bool) -> Option<CalculationContext> {
        let mut ctx = CalculationContext::new(c.pc.game, &c.bot.settings, false, threaded)?;
        ctx.allow_break = true;
        Some(ctx)
    }

    fn rescan_now(&mut self, c: &mut ProcessCtx, known: &[BlockPos]) {
        let Some(bom) = self.getting_to.clone() else { return };
        let Some(ctx) = Self::context(c, false) else {
            self.known_locations = Some(Vec::new());
            return;
        };
        let cache = c.bot.world.current().map(|w| w.cache.clone());
        let feet = c.pc.player_feet();
        let player_block = BlockPos::of_vec(c.pc.player.position);
        let filter = BlockOptionalMetaLookup::new(vec![bom]);
        let mut positions = search_world(
            &ctx,
            cache.as_deref(),
            &filter,
            MAX_LOCATIONS,
            known,
            &self.blacklist,
            &mut Vec::new(),
            feet,
            player_block,
        );
        positions.retain(|p| !self.blacklist.contains(p));
        self.known_locations = Some(positions);
    }

    fn rescan_in_background(&mut self, c: &ProcessCtx) {
        if self.rescanning {
            return;
        }
        let Some(bom) = self.getting_to.clone() else { return };
        let Some(ctx) = Self::context(c, true) else { return };
        let cache = c.bot.world.current().map(|w| w.cache.clone());
        let feet = c.pc.player_feet();
        let player_block = BlockPos::of_vec(c.pc.player.position);
        let known = self.known_locations.clone().unwrap_or_default();
        let blacklist = self.blacklist.clone();
        let slot = self.rescan.get_or_insert_with(worker::Slot::new).clone();
        self.rescanning = true;
        worker::execute(move || {
            let filter = BlockOptionalMetaLookup::new(vec![bom]);
            let mut positions = search_world(
                &ctx,
                cache.as_deref(),
                &filter,
                MAX_LOCATIONS,
                &known,
                &blacklist,
                &mut Vec::new(),
                feet,
                player_block,
            );
            positions.retain(|p| !blacklist.contains(p));
            slot.put(positions);
        });
    }

    fn take_rescan(&mut self) {
        if let Some(slot) = &self.rescan
            && let Some(positions) = slot.take()
        {
            self.rescanning = false;
            // Blacklisted since the scan started: still out.
            let bl = &self.blacklist;
            self.known_locations = Some(positions.into_iter().filter(|p| !bl.contains(p)).collect());
        }
    }

    /// `blacklistClosest`: the closest location and every location joined to it face to face.
    fn blacklist_closest(&mut self, c: &mut ProcessCtx) -> bool {
        let feet = c.pc.player_feet();
        let known = self.known_locations.get_or_insert_with(Vec::new);
        let mut new_blacklist: Vec<BlockPos> = known
            .iter()
            .copied()
            .min_by(|a, b| feet.dist_sqr(*a).total_cmp(&feet.dist_sqr(*b)))
            .into_iter()
            .collect();
        'outer: loop {
            for i in 0..known.len() {
                let k = known[i];
                if new_blacklist.iter().any(|b| are_adjacent(k, *b)) {
                    new_blacklist.push(k);
                    known.remove(i);
                    continue 'outer;
                }
            }
            break;
        }
        c.bot.log_debug(&format!("Blacklisting unreachable locations {new_blacklist:?}"));
        let any = !new_blacklist.is_empty();
        self.blacklist.extend(new_blacklist);
        any
    }

    /// `createGoal`.
    fn create_goal(&self, c: &ProcessCtx, pos: BlockPos) -> GoalRef {
        let name = self.getting_to.as_ref().map(|b| &*b.name).unwrap_or("");
        let s = &c.bot.settings;
        if walk_into_instead_of_adjacent(name, s.enter_portal) {
            return Arc::new(GoalTwoBlocks::at(pos));
        }
        // TODO this should be the check for whether the chest can open.
        if block_on_top_must_be_removed(name, s.right_click_container_on_arrival)
            && helper::is_block_normal_cube(c.pc.world.info_pos(pos.above()))
        {
            return Arc::new(GoalBlock::at(pos.above()));
        }
        Arc::new(GoalGetToBlock::at(pos))
    }

    /// `rightClick`: `true` once done, opened or not.
    fn right_click(&mut self, c: &mut ProcessCtx) -> bool {
        let known = self.known_locations.clone().unwrap_or_default();
        let reach = c.pc.block_reach_distance();
        for pos in &known {
            let Some(rot) = c.pc.reachable(c.bot.look.aim(), *pos, reach, false) else {
                continue;
            };
            c.bot
                .look
                .update_target(rot, true, &c.bot.settings, c.pc.player.fall_flying);
            if c.pc.aimed_block(&c.bot.look, Some(rot)).is_some_and(|p| known.contains(&p)) {
                // TODO find some way to right click even if we're in an ESC menu.
                c.bot.input.set_input_force_state(Input::ClickRight, true);
                if c.pc.game.container_open {
                    return true;
                }
            }
            let ticks = self.arrival_tick_count;
            self.arrival_tick_count += 1;
            if ticks > 20 {
                c.bot.log_direct("Right click timed out");
                return true;
            }
            // Trying to right click: it will happen this tick or the next.
            return false;
        }
        c.bot.log_direct("Arrived but failed to right click open");
        true
    }

    fn tick(&mut self, c: &mut ProcessCtx, calc_failed: bool, is_safe_to_cancel: bool) -> Option<PathingCommand> {
        if self.known_locations.is_none() {
            self.rescan_now(c, &[]);
        }
        let name = self.getting_to.as_ref().map(|b| b.to_string()).unwrap_or_default();
        let known = self.known_locations.clone().unwrap_or_default();
        if known.is_empty() {
            if c.bot.settings.explore_for_blocks && !calc_failed {
                let start = self.start.unwrap_or_else(|| c.pc.player_feet());
                return Some(PathingCommand::new(
                    Some(Arc::new(GoalExplore(GoalRunAway::new(1.0, None, vec![start])))),
                    PathingCommandType::ForceRevalidateGoalAndPath,
                ));
            }
            c.bot.log_direct(&format!("No known locations of {name}, canceling GetToBlock"));
            if is_safe_to_cancel {
                self.on_lost_control(c.bot);
            }
            return Some(PathingCommand::new(None, PathingCommandType::CancelAndSetGoal));
        }
        let goal: GoalRef = Arc::new(GoalComposite::new(known.iter().map(|p| self.create_goal(c, *p)).collect()));
        if calc_failed {
            if c.bot.settings.blacklist_closest_on_failure {
                c.bot.log_direct(&format!(
                    "Unable to find any path to {name}, blacklisting presumably unreachable closest instances..."
                ));
                self.blacklist_closest(c);
                return self.tick(c, false, is_safe_to_cancel); // gamer moment
            }
            c.bot.log_direct(&format!("Unable to find any path to {name}, canceling GetToBlock"));
            if is_safe_to_cancel {
                self.on_lost_control(c.bot);
            }
            return Some(PathingCommand::new(Some(goal), PathingCommandType::CancelAndSetGoal));
        }
        let interval = c.bot.settings.mine_goal_update_interval;
        let tc = self.tick_count;
        self.tick_count += 1;
        if interval != 0 && tc % interval == 0 {
            self.rescan_in_background(c);
        }
        let feet = c.pc.player_feet();
        if goal.is_in_goal_pos(feet) && goal.is_in_goal_pos(PathingBehavior::path_start(c.pc)) && is_safe_to_cancel {
            // We're there.
            let open = right_click_on_arrival(
                self.getting_to.as_ref().map(|b| &*b.name).unwrap_or(""),
                c.bot.settings.right_click_container_on_arrival,
            );
            if !open || self.right_click(c) {
                self.on_lost_control(c.bot);
                return Some(PathingCommand::new(None, PathingCommandType::CancelAndSetGoal));
            }
        }
        Some(PathingCommand::new(Some(goal), PathingCommandType::RevalidateGoalAndPath))
    }
}

/// Directly adjacent (face to face).
fn are_adjacent(a: BlockPos, b: BlockPos) -> bool {
    (a.x - b.x).abs() + (a.y - b.y).abs() + (a.z - b.z).abs() == 1
}

/// `walkIntoInsteadOfAdjacent`.
fn walk_into_instead_of_adjacent(block: &str, enter_portal: bool) -> bool {
    enter_portal && block == "minecraft:nether_portal"
}

/// `rightClickOnArrival`.
fn right_click_on_arrival(block: &str, setting: bool) -> bool {
    setting
        && matches!(
            block,
            "minecraft:crafting_table"
                | "minecraft:furnace"
                | "minecraft:blast_furnace"
                | "minecraft:ender_chest"
                | "minecraft:chest"
                | "minecraft:trapped_chest"
        )
}

/// `blockOnTopMustBeRemoved`: only chests (a crafting table or furnace opens with a block on top), and only
/// when they will be opened.
fn block_on_top_must_be_removed(block: &str, setting: bool) -> bool {
    right_click_on_arrival(block, setting)
        && matches!(block, "minecraft:ender_chest" | "minecraft:chest" | "minecraft:trapped_chest")
}

impl Process for GetToBlockProcess {
    fn is_active(&mut self, _c: &mut ProcessCtx) -> bool {
        self.getting_to.is_some()
    }

    fn on_tick(&mut self, c: &mut ProcessCtx, calc_failed: bool, is_safe_to_cancel: bool) -> Option<PathingCommand> {
        self.take_rescan();
        self.tick(c, calc_failed, is_safe_to_cancel)
    }

    fn on_lost_control(&mut self, bot: &mut Bot) {
        self.getting_to = None;
        self.known_locations = None;
        self.start = None;
        self.blacklist.clear();
        self.rescan = None;
        self.rescanning = false;
        bot.input.clear_all_keys();
    }

    fn display_name(&self) -> String {
        let name = self.getting_to.as_ref().map(|b| b.to_string()).unwrap_or_default();
        match &self.known_locations {
            Some(k) if !k.is_empty() => format!("Get To {name}, {} known locations", k.len()),
            _ => format!("Exploring randomly to find {name}, no known locations"),
        }
    }
}
