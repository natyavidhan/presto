//! `FollowProcess`: follow entities (`#follow`), or walk into dropped items (`#pickup`).

use super::{PathingCommand, PathingCommandType, Process, ProcessCtx};
use crate::bot::Bot;
use crate::math::BlockPos;
use crate::pathing::goals::{GoalBlock, GoalComposite, GoalNear, GoalRef, GoalXZ};
use crate::player::EntityInfo;
use std::sync::Arc;

/// What to follow (`Predicate<Entity>`).
#[derive(Clone, Debug, PartialEq)]
pub enum FollowFilter {
    /// Every entity.
    Entities,
    /// Every player.
    Players,
    /// Players by name (ignoring case).
    PlayerNames(Vec<String>),
    /// Entities by type (`minecraft:cow`).
    EntityTypes(Vec<String>),
    /// One entity.
    EntityId(i32),
    /// Dropped items, by item id (empty for any item): `#pickup`.
    Items(Vec<String>),
}

impl FollowFilter {
    fn matches(&self, e: &EntityInfo) -> bool {
        match self {
            FollowFilter::Entities => e.living,
            FollowFilter::Players => e.is_player,
            FollowFilter::PlayerNames(names) => {
                e.is_player
                    && e
                        .name
                        .as_deref()
                        .is_some_and(|n| names.iter().any(|w| w.eq_ignore_ascii_case(n)))
            }
            FollowFilter::EntityTypes(kinds) => kinds.iter().any(|k| **k == *e.kind),
            FollowFilter::EntityId(id) => e.id == *id,
            FollowFilter::Items(items) => {
                e.item
                    .as_deref()
                    .is_some_and(|i| items.is_empty() || items.iter().any(|w| w == i))
            }
        }
    }
}

#[derive(Default)]
pub struct FollowProcess {
    filter: Option<FollowFilter>,
    cache: Vec<EntityInfo>,
    /// Walk straight into the target whatever the settings say (`pickup`).
    into: bool,
}

impl FollowProcess {
    /// `follow`.
    pub fn follow(&mut self, filter: FollowFilter) {
        self.filter = Some(filter);
        self.into = false;
    }

    /// `pickup`.
    pub fn pickup(&mut self, items: Vec<String>) {
        self.filter = Some(FollowFilter::Items(items));
        self.into = true;
    }

    pub fn following(&self) -> &[EntityInfo] {
        &self.cache
    }

    pub fn current_filter(&self) -> Option<&FollowFilter> {
        self.filter.as_ref()
    }

    /// `scanWorld`.
    fn scan_world(&mut self, c: &ProcessCtx) {
        let Some(filter) = &self.filter else {
            self.cache.clear();
            return;
        };
        let me = c.pc.player;
        let max = c.bot.settings.follow_target_max_distance;
        self.cache = c
            .pc
            .game
            .entities
            .iter()
            .filter(|e| e.alive && e.id != me.entity_id)
            .filter(|e| {
                max == 0 || e.position.distance_to_sqr(me.position) <= (max as f64) * (max as f64)
            })
            .filter(|e| filter.matches(e))
            .cloned()
            .collect();
    }

    /// `towards`.
    fn towards(&self, c: &ProcessCtx, e: &EntityInfo) -> GoalRef {
        let s = &c.bot.settings;
        let pos = if s.follow_offset_distance == 0.0 || self.into {
            BlockPos::of_vec(e.position)
        } else {
            let g = GoalXZ::from_direction(e.position, s.follow_offset_direction, s.follow_offset_distance);
            BlockPos::new(g.x, e.position.y.floor() as i32, g.z)
        };
        if self.into {
            Arc::new(GoalBlock::at(pos))
        } else {
            Arc::new(GoalNear::new(pos, s.follow_radius))
        }
    }
}

impl Process for FollowProcess {
    fn is_active(&mut self, c: &mut ProcessCtx) -> bool {
        if self.filter.is_none() {
            return false;
        }
        self.scan_world(c);
        !self.cache.is_empty()
    }

    fn on_tick(&mut self, c: &mut ProcessCtx, _calc_failed: bool, _is_safe_to_cancel: bool) -> Option<PathingCommand> {
        self.scan_world(c);
        let goals: Vec<GoalRef> = self.cache.iter().map(|e| self.towards(c, e)).collect();
        Some(PathingCommand::new(
            Some(Arc::new(GoalComposite::new(goals))),
            PathingCommandType::RevalidateGoalAndPath,
        ))
    }

    fn on_lost_control(&mut self, _bot: &mut Bot) {
        self.filter = None;
        self.cache.clear();
    }

    fn display_name(&self) -> String {
        let names: Vec<String> = self
            .cache
            .iter()
            .map(|e| e.name.as_deref().map(str::to_string).unwrap_or_else(|| e.kind.to_string()))
            .collect();
        format!("Following [{}]", names.join(", "))
    }
}
