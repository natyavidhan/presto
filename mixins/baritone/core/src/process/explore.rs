//! `ExploreProcess`: walk to the nearest chunks the cache doesn't have (`#explore`). Ported with the commands.

use super::{PathingCommand, Process, ProcessCtx};
use crate::bot::Bot;

#[derive(Default)]
pub struct ExploreProcess {}

impl Process for ExploreProcess {
    fn is_active(&mut self, _c: &mut ProcessCtx) -> bool {
        false
    }

    fn on_tick(&mut self, _c: &mut ProcessCtx, _calc_failed: bool, _is_safe_to_cancel: bool) -> Option<PathingCommand> {
        None
    }

    fn on_lost_control(&mut self, _bot: &mut Bot) {}

    fn display_name(&self) -> String {
        "Exploring".into()
    }
}
