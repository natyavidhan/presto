//! The process behind `#pause` and `#resume` (`ExecutionControlCommands`): while paused, it holds everything
//! still, and lets go of nothing.

use super::{DEFAULT_PRIORITY, PathingCommand, PathingCommandType, Process, ProcessCtx};
use crate::bot::Bot;

#[derive(Default)]
pub struct PauseProcess {
    pub paused: bool,
}

impl Process for PauseProcess {
    fn is_active(&mut self, _c: &mut ProcessCtx) -> bool {
        self.paused
    }

    fn on_tick(&mut self, c: &mut ProcessCtx, _calc_failed: bool, _is_safe_to_cancel: bool) -> Option<PathingCommand> {
        c.bot.input.clear_all_keys();
        Some(PathingCommand::new(None, PathingCommandType::RequestPause))
    }

    fn is_temporary(&self) -> bool {
        true
    }

    fn on_lost_control(&mut self, _bot: &mut Bot) {}

    fn priority(&self) -> f64 {
        DEFAULT_PRIORITY + 1.0
    }

    fn display_name(&self) -> String {
        "Pause/Resume Commands".into()
    }
}
