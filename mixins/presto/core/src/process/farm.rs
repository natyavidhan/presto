//! `FarmProcess`: harvest and replant crops (`#farm`). Ported with the commands.

use super::{PathingCommand, Process, ProcessCtx};
use crate::bot::Bot;

#[derive(Default)]
pub struct FarmProcess {}

impl Process for FarmProcess {
    fn is_active(&mut self, _c: &mut ProcessCtx) -> bool {
        false
    }

    fn on_tick(&mut self, _c: &mut ProcessCtx, _calc_failed: bool, _is_safe_to_cancel: bool) -> Option<PathingCommand> {
        None
    }

    fn on_lost_control(&mut self, _bot: &mut Bot) {}

    fn display_name(&self) -> String {
        "Farming".into()
    }
}
