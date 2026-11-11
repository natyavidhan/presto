//! `BuilderProcess`: building schematics (`#build`, `#sel fill`...). For now this holds what the rest of the
//! bot uses from it; the process itself comes with the schematic formats.

use super::{PathingCommand, Process, ProcessCtx};
use crate::bot::Bot;
use crate::ctx::PlayerContext;
use crate::math::{Aabb, BlockPos};

/// `placementPlausible`: nothing that stops building (a mob, a player, a boat...) is in the shape the block
/// would have at `pos` (block-local boxes; an empty shape always fits).
pub fn placement_plausible(pc: &PlayerContext, pos: BlockPos, shape: &[Aabb]) -> bool {
    if shape.is_empty() {
        return true;
    }
    let (x, y, z) = (pos.x as f64, pos.y as f64, pos.z as f64);
    let blocked = |bb: Aabb| shape.iter().any(|s| s.moved(x, y, z).intersects(bb));
    if blocked(pc.player.bounding_box) {
        return false;
    }
    !pc
        .game
        .entities
        .iter()
        .any(|e| e.alive && e.blocks_building && e.id != pc.player.entity_id && blocked(e.bounding_box))
}

#[derive(Default)]
pub struct BuilderProcess {}

impl BuilderProcess {
    /// `placeAt`: the block the schematic wants at a position, while building.
    pub fn place_at(&self, _x: i32, _y: i32, _z: i32) -> Option<String> {
        None
    }

    /// `clearArea`: mine out everything between two corners.
    pub fn clear_area(&mut self, bot: &mut Bot, _corner1: crate::math::BlockPos, _corner2: crate::math::BlockPos) {
        bot.log_direct("Clearing an area needs the builder, which isn't ported yet");
    }

    /// `resume`: carry on after `pause`.
    pub fn resume(&mut self) {}
}

impl Process for BuilderProcess {
    fn is_active(&mut self, _c: &mut ProcessCtx) -> bool {
        false
    }

    fn on_tick(&mut self, _c: &mut ProcessCtx, _calc_failed: bool, _is_safe_to_cancel: bool) -> Option<PathingCommand> {
        None
    }

    fn on_lost_control(&mut self, _bot: &mut Bot) {}

    fn display_name(&self) -> String {
        "Builder".into()
    }
}
