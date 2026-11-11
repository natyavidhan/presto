//! `BackfillProcess` (`backfill`): puts back the blocks the path broke once the player is past them, so tunnels
//! don't stay open behind it.

use super::builder::placement_plausible;
use super::{PathingCommand, PathingCommandType, Process, ProcessCtx};
use crate::behavior::input::Input;
use crate::bot::Bot;
use crate::math::{Aabb, BlockPos};
use crate::pathing::movement::{MovementState, PlaceResult, attempt_to_place_a_block};
use crate::world::StateId;
use rustc_hash::FxHashMap;

#[derive(Default)]
pub struct BackfillProcess {
    /// Blocks the crosshair was on while pathing (`blocksToReplace`), until they are put back or replaced.
    pub blocks_to_replace: FxHashMap<BlockPos, StateId>,
}

impl BackfillProcess {
    /// `amIBreakingABlockHMMMMMMM`.
    fn remember_target(&mut self, c: &ProcessCtx) {
        if let Some(pos) = c.pc.selected_block()
            && c.bot.pathing.is_pathing()
        {
            self.blocks_to_replace.insert(pos, c.pc.world.get_pos(pos));
        }
    }

    /// `toFillIn`: broken (air now), with room for a block, and not part of the movement under way; the
    /// farthest first.
    pub fn to_fill_in(&self, c: &ProcessCtx) -> Vec<BlockPos> {
        let world = &c.pc.world;
        // Placement is checked as for dirt: a full cube.
        let cube = [Aabb::new(0.0, 0.0, 0.0, 1.0, 1.0, 1.0)];
        let current = c
            .bot
            .pathing
            .current()
            .filter(|e| !e.finished() && !e.failed())
            .and_then(|e| e.path().movements().get(e.position()));
        let feet = c.pc.player_feet();
        let mut out: Vec<BlockPos> = self
            .blocks_to_replace
            .keys()
            .copied()
            .filter(|p| world.info_pos(*p).is_air())
            .filter(|p| placement_plausible(c.pc, *p, &cube))
            .filter(|p| current.is_none_or(|m| !m.positions_to_break.contains(p)))
            .collect();
        out.sort_by(|a, b| feet.dist_sqr(*b).total_cmp(&feet.dist_sqr(*a)));
        out
    }
}

impl Process for BackfillProcess {
    fn is_active(&mut self, c: &mut ProcessCtx) -> bool {
        if !c.bot.settings.backfill {
            return false;
        }
        if c.bot.settings.allow_parkour {
            c.bot.log_direct("Backfill cannot be used with allowParkour true");
            c.bot.settings_mut().backfill = false;
            return false;
        }
        let world = &c.pc.world;
        self.blocks_to_replace
            .retain(|p, _| world.is_loaded(p.x, p.z) && world.info_pos(*p).is_air());
        self.remember_target(c);
        c.bot.input.clear_all_keys();
        !self.to_fill_in(c).is_empty()
    }

    fn on_tick(&mut self, c: &mut ProcessCtx, _calc_failed: bool, is_safe_to_cancel: bool) -> Option<PathingCommand> {
        if !is_safe_to_cancel {
            return Some(PathingCommand::new(None, PathingCommandType::RequestPause));
        }
        c.bot.input.clear_all_keys();
        for to_place in self.to_fill_in(c) {
            let mut fake = MovementState::default();
            let result = c.with_exec(|ex| attempt_to_place_a_block(&mut fake, ex, to_place, false, false));
            let fall_flying = c.pc.player.fall_flying;
            match result {
                PlaceResult::NoOption => continue,
                PlaceResult::ReadyToPlace => {
                    // With instantAim, "ready" can mean ready once this tick's turn is made.
                    if c.bot.settings.instant_aim
                        && let Some(rot) = fake.target.rotation
                    {
                        c.bot.look.update_target(rot, true, &c.bot.settings, fall_flying);
                    }
                    c.bot.input.set_input_force_state(Input::ClickRight, true);
                    return Some(PathingCommand::new(None, PathingCommandType::RequestPause));
                }
                PlaceResult::Attempting => {
                    // Patience.
                    if let Some(rot) = fake.target.rotation {
                        c.bot.look.update_target(rot, true, &c.bot.settings, fall_flying);
                    }
                    return Some(PathingCommand::new(None, PathingCommandType::RequestPause));
                }
            }
        }
        // Cede to the other processes.
        Some(PathingCommand::new(None, PathingCommandType::Defer))
    }

    fn on_lost_control(&mut self, _bot: &mut Bot) {
        self.blocks_to_replace.clear();
    }

    fn is_temporary(&self) -> bool {
        true
    }

    fn priority(&self) -> f64 {
        5.0
    }

    fn display_name(&self) -> String {
        "Backfill".into()
    }
}
