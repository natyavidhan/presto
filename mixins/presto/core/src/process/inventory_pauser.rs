//! `InventoryPauserProcess`: comes to a stop for an inventory move when `inventoryMoveOnlyIfStationary` asks.

use super::{PathingCommand, PathingCommandType, Process, ProcessCtx};
use crate::bot::Bot;

#[derive(Default)]
pub struct InventoryPauserProcess;

impl Process for InventoryPauserProcess {
    fn is_active(&mut self, c: &mut ProcessCtx) -> bool {
        c.pc.game.in_world()
    }

    fn on_tick(&mut self, c: &mut ProcessCtx, _calc_failed: bool, is_safe_to_cancel: bool) -> Option<PathingCommand> {
        let p = &mut c.bot.pauser;
        p.safe_to_cancel_last_tick = is_safe_to_cancel;
        if p.pause_requested_last_tick {
            p.pause_requested_last_tick = false;
            let v = c.pc.player.velocity;
            if (v.x * v.x + v.z * v.z).sqrt() < 0.00001 {
                p.ticks_of_stationary += 1;
            }
            return Some(PathingCommand::new(None, PathingCommandType::RequestPause));
        }
        p.ticks_of_stationary = 0;
        Some(PathingCommand::new(None, PathingCommandType::Defer))
    }

    fn is_temporary(&self) -> bool {
        true
    }

    fn on_lost_control(&mut self, _bot: &mut Bot) {}

    /// Slightly higher than backfill.
    fn priority(&self) -> f64 {
        5.1
    }

    fn display_name(&self) -> String {
        "inventory pauser".into()
    }
}
