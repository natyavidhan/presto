//! `ExploreProcess`: walk to the nearest chunks Baritone's cache doesn't have yet (`#explore`), optionally with a
//! list of chunks already explored, or still to explore, from a JSON file (`#explorefilter`).

use super::{PathingCommand, PathingCommandType, Process, ProcessCtx};
use crate::bot::Bot;
use crate::cache::CachedWorld;
use crate::math::BlockPos;
use crate::pathing::goals::{Goal, GoalComposite, GoalRef, GoalXZ, GoalYLevel};
use crate::settings::Settings;
use rustc_hash::FxHashSet;
use std::any::Any;
use std::fmt;
use std::sync::Arc;

/// The `GoalXZ` explore uses while `exploreMaintainY` is set: in the goal at the column, at any height, but
/// keeping to that height on the way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GoalXZMaintainY {
    pub xz: GoalXZ,
    pub y: i32,
}

impl Goal for GoalXZMaintainY {
    fn is_in_goal(&self, x: i32, y: i32, z: i32) -> bool {
        self.xz.is_in_goal(x, y, z)
    }
    fn heuristic(&self, x: i32, y: i32, z: i32) -> f64 {
        self.xz.heuristic(x, y, z) + GoalYLevel::calculate(self.y, y)
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn same_as(&self, other: &dyn Goal) -> bool {
        other.as_any().downcast_ref::<Self>().is_some_and(|o| o == self)
    }
}

impl fmt::Display for GoalXZMaintainY {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.xz, f)
    }
}

fn create_goal(settings: &Settings, x: i32, z: i32) -> GoalRef {
    let xz = GoalXZ::new(x, z);
    if settings.explore_maintain_y == -1 {
        Arc::new(xz)
    } else {
        // Not a GoalBlock: it's in the goal at any height; the height is only kept to on the way.
        Arc::new(GoalXZMaintainY {
            xz,
            y: settings.explore_maintain_y,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Status {
    Explored,
    NotExplored,
    /// The region has to be read from disk first.
    Unknown,
}

/// `JsonChunkFilter`: chunks listed as explored (or, inverted, as not explored).
#[derive(Clone, Debug)]
pub struct ChunkFilter {
    invert: bool,
    chunks: FxHashSet<(i32, i32)>,
    positions: Vec<(i32, i32)>,
}

impl ChunkFilter {
    /// Reads `[{"x":0,"z":0},...]`.
    pub fn parse(json: &str, invert: bool) -> Result<ChunkFilter, String> {
        let positions = parse_chunk_list(json)?;
        Ok(ChunkFilter {
            invert,
            chunks: positions.iter().copied().collect(),
            positions,
        })
    }

    pub fn len(&self) -> usize {
        self.positions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }
}

/// A list of `{"x": .., "z": ..}` objects, the only JSON an explore filter holds.
fn parse_chunk_list(json: &str) -> Result<Vec<(i32, i32)>, String> {
    let s = json.trim();
    let inner = s
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or("Invalid JSON syntax")?;
    let mut out = Vec::new();
    for object in inner.split('}') {
        let object = object.trim().trim_start_matches(',').trim();
        if object.is_empty() {
            continue;
        }
        let body = object.strip_prefix('{').ok_or("Invalid JSON syntax")?;
        let (mut x, mut z) = (None, None);
        for field in body.split(',') {
            let Some((k, v)) = field.split_once(':') else {
                if field.trim().is_empty() {
                    continue;
                }
                return Err("Invalid JSON syntax".into());
            };
            let k = k.trim().trim_matches('"');
            let v: f64 = v.trim().parse().map_err(|_| "Invalid JSON syntax".to_string())?;
            match k {
                "x" => x = Some(v as i32),
                "z" => z = Some(v as i32),
                _ => {}
            }
        }
        out.push((x.unwrap_or(0), z.unwrap_or(0)));
    }
    Ok(out)
}

#[derive(Default)]
pub struct ExploreProcess {
    origin: Option<BlockPos>,
    filter: Option<ChunkFilter>,
    distance_completed: i32,
}

/// `BaritoneChunkCache.isAlreadyExplored`.
fn cache_status(cache: &Arc<CachedWorld>, cx: i32, cz: i32) -> Status {
    let (x, z) = (cx << 4, cz << 4);
    if cache.is_cached(x, z) {
        return Status::Explored;
    }
    if !cache.region_loaded(x, z) {
        let cache = cache.clone();
        crate::utils::worker::execute(move || cache.try_load_from_disk(x >> 9, z >> 9));
        return Status::Unknown;
    }
    Status::NotExplored
}

impl ExploreProcess {
    /// `explore`.
    pub fn explore(&mut self, x: i32, z: i32) {
        self.origin = Some(BlockPos::new(x, 0, z));
        self.distance_completed = 0;
    }

    /// `applyJsonFilter`.
    pub fn apply_filter(&mut self, filter: ChunkFilter) {
        self.filter = Some(filter);
    }

    /// `calcFilter`'s `isAlreadyExplored`: the JSON filter's say, then the cache's.
    fn status(&self, cache: &Arc<CachedWorld>, cx: i32, cz: i32) -> Status {
        if let Some(f) = &self.filter {
            if f.chunks.contains(&(cx, cz)) ^ f.invert {
                return Status::Explored;
            }
            // Either way it depends on whether the cache has it (EitherChunk falls to the cache).
        }
        cache_status(cache, cx, cz)
    }

    /// `countRemain`: unbounded unless an inverted filter lists what's left.
    fn count_remain(&self, cache: &Arc<CachedWorld>, settings: &Settings) -> usize {
        let Some(f) = &self.filter else {
            return usize::MAX;
        };
        if !f.invert {
            return usize::MAX;
        }
        let min = settings.explore_chunk_set_minimum_size.max(0) as usize;
        let mut remain = 0;
        for &(x, z) in &f.positions {
            if cache_status(cache, x, z) != Status::Explored {
                remain += 1;
                if remain >= min {
                    return remain;
                }
            }
        }
        remain
    }

    /// `closestUncachedChunks`: `None` while regions are read from disk.
    fn closest_uncached(&mut self, cache: &Arc<CachedWorld>, settings: &Settings) -> Option<Vec<GoalRef>> {
        let origin = self.origin?;
        let (chunk_x, chunk_z) = (origin.x >> 4, origin.z >> 4);
        let min = settings.explore_chunk_set_minimum_size.max(0) as usize;
        let mut count = self.count_remain(cache, settings).min(min);
        let offset = settings.world_exploring_chunk_offset << 4;
        let mut centers: Vec<(i32, i32)> = Vec::new();
        let mut dist = self.distance_completed;
        // Bounded so a filter that marks everything explored can't spin forever.
        while dist < self.distance_completed + 100_000 {
            for dx in -dist..=dist {
                let zval = dist - dx.abs();
                for mult in 0..2 {
                    let dz = (mult * 2 - 1) * zval;
                    match self.status(cache, chunk_x + dx, chunk_z + dz) {
                        Status::Unknown => return None,
                        Status::Explored => continue,
                        Status::NotExplored => {}
                    }
                    let mut x = ((chunk_x + dx) << 4) + 8;
                    let mut z = ((chunk_z + dz) << 4) + 8;
                    x += if dx < 0 { -offset } else { offset };
                    z += if dz < 0 { -offset } else { offset };
                    centers.push((x, z));
                }
            }
            if dist % 10 == 0 {
                count = self.count_remain(cache, settings).min(min);
            }
            if centers.len() >= count {
                return Some(centers.into_iter().map(|(x, z)| create_goal(settings, x, z)).collect());
            }
            if centers.is_empty() {
                // Everything from 0 to dist is explored: start after it next time.
                self.distance_completed = dist + 1;
            }
            dist += 1;
        }
        Some(centers.into_iter().map(|(x, z)| create_goal(settings, x, z)).collect())
    }
}

impl Process for ExploreProcess {
    fn is_active(&mut self, _c: &mut ProcessCtx) -> bool {
        self.origin.is_some()
    }

    fn on_tick(&mut self, c: &mut ProcessCtx, calc_failed: bool, _is_safe_to_cancel: bool) -> Option<PathingCommand> {
        if calc_failed {
            c.bot.log_direct("Failed");
            if c.bot.settings.notification_on_explore_finished {
                c.bot.log_notification("Exploration failed", true);
            }
            self.on_lost_control(c.bot);
            return None;
        }
        let Some(cache) = c.bot.world.current().map(|w| w.cache.clone()) else {
            return Some(PathingCommand::new(None, PathingCommandType::RequestPause));
        };
        let settings = c.bot.settings.clone();
        if !settings.disable_completion_check && self.count_remain(&cache, &settings) == 0 {
            c.bot.log_direct("Explored all chunks");
            if settings.notification_on_explore_finished {
                c.bot.log_notification("Explored all chunks", false);
            }
            self.on_lost_control(c.bot);
            return None;
        }
        match self.closest_uncached(&cache, &settings) {
            None => {
                c.bot.log_debug("awaiting region load from disk");
                Some(PathingCommand::new(None, PathingCommandType::RequestPause))
            }
            Some(goals) => Some(PathingCommand::new(
                Some(Arc::new(GoalComposite::new(goals))),
                PathingCommandType::ForceRevalidateGoalAndPath,
            )),
        }
    }

    fn on_lost_control(&mut self, _bot: &mut Bot) {
        self.origin = None;
    }

    fn display_name(&self) -> String {
        match self.origin {
            Some(o) => format!(
                "Exploring around {}, distance completed {}",
                crate::command::defaults::pos_string(o),
                self.distance_completed
            ),
            None => "Exploring".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_filter_files() {
        let f = ChunkFilter::parse(r#"[{"x":0,"z":0}, {"x": -3, "z": 12} ,{"z":5,"x":2}]"#, false).unwrap();
        assert_eq!(f.positions, vec![(0, 0), (-3, 12), (2, 5)]);
        assert!(ChunkFilter::parse("[]", true).unwrap().is_empty());
        assert!(ChunkFilter::parse("{", false).is_err());
    }
}
