//! `ElytraProcess`: elytra flight through the nether (`#elytra`). Ported after the rest.

use super::{PathingCommand, Process, ProcessCtx};
use crate::bot::Bot;

#[derive(Default)]
pub struct ElytraProcess {}

impl ElytraProcess {
    /// Flying, or taking off (`isActive` without the setup checks).
    pub fn is_active_now(&self) -> bool {
        false
    }

    /// `isSafeToCancel`: not in the air.
    pub fn is_safe_to_cancel(&self) -> bool {
        true
    }
}

impl Process for ElytraProcess {
    fn is_active(&mut self, _c: &mut ProcessCtx) -> bool {
        false
    }

    fn on_tick(&mut self, _c: &mut ProcessCtx, _calc_failed: bool, _is_safe_to_cancel: bool) -> Option<PathingCommand> {
        None
    }

    fn on_lost_control(&mut self, _bot: &mut Bot) {}

    fn priority(&self) -> f64 {
        0.0
    }

    fn display_name(&self) -> String {
        "Elytra".into()
    }
}
