//! The `IDatatype`s: how command words are read as coordinates, goals, blocks, items, entities, players and
//! waypoints, and how they're completed. Each reader takes what it needs from [`Args`]; wrap it in
//! [`Args::datatype`] to report a failure the way Baritone does.

use super::args::{Args, is_plain_decimal, word};
use super::helpers::Completions;
use crate::cache::waypoints::{Tag, Waypoint, WaypointCollection};
use crate::math::{BlockPos, Direction, mth};
use crate::pathing::goals::{GoalBlock, GoalRef, GoalXZ, GoalYLevel};
use crate::player::{EntityInfo, GameState};
use crate::settings::normalize_id;
use crate::utils::block_optional_meta::BlockOptionalMeta;
use crate::world::{BlockId, BlockTable};
use std::sync::Arc;

/// `RelativeCoordinate`: `5`, `-2.5`, `~` (the origin), `~-3`, and `k`/`m` for thousands and millions
/// (`~1.5k`). An empty word is 0.
pub fn parse_relative_coordinate(s: &str, origin: f64) -> Option<f64> {
    let (relative, rest) = match s.strip_prefix('~') {
        Some(r) => (true, r),
        None => (false, s),
    };
    let offset = if rest.is_empty() {
        0.0
    } else {
        let (num, mult) = match rest.as_bytes()[rest.len() - 1] {
            b'k' | b'K' => (&rest[..rest.len() - 1], 1_000.0),
            b'm' | b'M' => (&rest[..rest.len() - 1], 1_000_000.0),
            _ => (rest, 1.0),
        };
        if !is_plain_decimal(num) {
            return None;
        }
        num.parse::<f64>().ok()? * mult
    };
    Some(if relative { origin + offset } else { offset })
}

pub fn relative_coordinate(args: &mut Args, origin: f64) -> Result<f64, String> {
    let s = word(args)?;
    parse_relative_coordinate(&s, origin).ok_or_else(|| format!("{s:?} isn't a coordinate"))
}

/// `RelativeCoordinate.tabComplete`: `~` for a last word that is empty or `~`.
pub fn complete_relative_coordinate(args: &mut Args) -> Vec<String> {
    if !args.has(2)
        && let Ok(s) = args.get_string()
        && (s.is_empty() || s == "~")
    {
        return vec!["~".into()];
    }
    Vec::new()
}

/// `RelativeBlockPos`: three coordinates.
pub fn relative_block_pos(args: &mut Args, origin: BlockPos) -> Result<BlockPos, String> {
    let x = relative_coordinate(args, origin.x as f64)?;
    let y = relative_coordinate(args, origin.y as f64)?;
    let z = relative_coordinate(args, origin.z as f64)?;
    Ok(BlockPos::of(x, y, z))
}

/// `RelativeBlockPos.tabComplete`.
pub fn complete_relative_block_pos(args: &mut Args) -> Vec<String> {
    if args.has_any() && !args.has(4) {
        while args.has(2) {
            if args.peek_datatype_or_none(|a| relative_coordinate(a, 0.0)).is_none() {
                break;
            }
            let _ = args.get();
        }
        return complete_relative_coordinate(args);
    }
    Vec::new()
}

/// `RelativeGoalBlock`.
pub fn relative_goal_block(args: &mut Args, origin: BlockPos) -> Result<GoalBlock, String> {
    let x = relative_coordinate(args, origin.x as f64)?;
    let y = relative_coordinate(args, origin.y as f64)?;
    let z = relative_coordinate(args, origin.z as f64)?;
    Ok(GoalBlock::new(mth::floor(x), mth::floor(y), mth::floor(z)))
}

/// `RelativeGoalXZ`.
pub fn relative_goal_xz(args: &mut Args, origin: BlockPos) -> Result<GoalXZ, String> {
    let x = relative_coordinate(args, origin.x as f64)?;
    let z = relative_coordinate(args, origin.z as f64)?;
    Ok(GoalXZ::new(mth::floor(x), mth::floor(z)))
}

/// `RelativeGoalYLevel`.
pub fn relative_goal_y_level(args: &mut Args, origin: BlockPos) -> Result<GoalYLevel, String> {
    let y = relative_coordinate(args, origin.y as f64)?;
    Ok(GoalYLevel::new(mth::floor(y)))
}

/// `RelativeGoal`: `x y z`, `x z` or `y`, whichever the words make; the origin itself with none. Like
/// Baritone's, it only looks at the words, it doesn't read them.
pub fn relative_goal(args: &mut Args, origin: BlockPos) -> Result<GoalRef, String> {
    if let Some(g) = args.peek_datatype_or_none(|a| relative_goal_block(a, origin)) {
        return Ok(Arc::new(g));
    }
    if let Some(g) = args.peek_datatype_or_none(|a| relative_goal_xz(a, origin)) {
        return Ok(Arc::new(g));
    }
    if let Some(g) = args.peek_datatype_or_none(|a| relative_goal_y_level(a, origin)) {
        return Ok(Arc::new(g));
    }
    Ok(Arc::new(GoalBlock::at(origin)))
}

/// `BlockById`.
pub fn block_by_id(table: &BlockTable, args: &mut Args) -> Result<BlockId, String> {
    let s = word(args)?;
    table
        .block_id(&normalize_id(&s))
        .ok_or_else(|| "no block found by that id".into())
}

/// `BlockById.tabComplete`.
pub fn complete_block_id(table: &BlockTable, args: &mut Args) -> Vec<String> {
    let Ok(s) = args.get_string() else { return Vec::new() };
    Completions::from(table.blocks().iter().map(|b| b.name.to_string()))
        .filter_prefix_namespaced(&s)
        .sort_alphabetically()
        .build()
}

/// `ForBlockOptionalMeta`: `stone`, `oak_log[axis=y]`...
pub fn block_optional_meta(table: &BlockTable, args: &mut Args) -> Result<BlockOptionalMeta, String> {
    let s = word(args)?;
    BlockOptionalMeta::parse(table, &s)
}

/// `ForBlockOptionalMeta.tabComplete`: the block id, then its properties and their values.
pub fn complete_block_optional_meta(table: &BlockTable, args: &mut Args) -> Vec<String> {
    let Ok(arg) = args.peek_string(0).map(str::to_string) else {
        return Vec::new();
    };
    let valid = arg
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "_-./:[]=,".contains(c));
    if !valid || arg.ends_with(']') {
        let _ = args.get();
        return Vec::new();
    }
    let Some(open) = arg.rfind('[') else {
        return complete_block_id(table, args);
    };
    let _ = args.get();
    let (block_id, properties) = (&arg[..open], &arg[open + 1..]);
    let Some(block) = table.block_id(&normalize_id(block_id)) else {
        return Vec::new();
    };
    let def = table.block(block);
    let base = table.get(def.default_state);
    let (leading, last) = match properties.rfind(',') {
        Some(i) => (&properties[..i], &properties[i + 1..]),
        None => ("", properties),
    };
    match last.split_once('=') {
        None => {
            // Completing a property's name.
            let used: Vec<&str> = leading.split(',').map(|p| p.split('=').next().unwrap_or("")).collect();
            let prefix = &arg[..arg.len() - last.len()];
            Completions::from(base.properties.iter().map(|(k, _)| k.to_string()))
                .filter(|p| !used.contains(&p))
                .filter_prefix(last)
                .sort_alphabetically()
                .map(|p| format!("{prefix}{p}"))
                .build()
        }
        Some((name, value)) => {
            // Completing its value.
            if base.property(name).is_none() {
                return Vec::new();
            }
            let mut values: Vec<String> = Vec::new();
            for s in &def.states {
                if let Some(v) = table.get(*s).property(name)
                    && !values.iter().any(|x| x == v)
                {
                    values.push(v.to_string());
                }
            }
            let prefix = &arg[..arg.len() - value.len()];
            Completions::from(values)
                .filter_prefix(value)
                .sort_alphabetically()
                .map(|v| format!("{prefix}{v}"))
                .build()
        }
    }
}

/// `ItemById`.
pub fn item_by_id(game: &GameState, args: &mut Args) -> Result<Arc<str>, String> {
    let id = normalize_id(&word(args)?);
    game.registries
        .items
        .iter()
        .find(|i| ***i == *id)
        .cloned()
        .ok_or_else(|| "No item found by that id".into())
}

pub fn complete_item_id(game: &GameState, args: &mut Args) -> Vec<String> {
    let Ok(s) = args.get_string() else { return Vec::new() };
    Completions::from(game.registries.items.iter().map(|i| i.to_string()))
        .filter_prefix_namespaced(&s)
        .sort_alphabetically()
        .build()
}

/// `EntityClassById`: an entity type.
pub fn entity_type_by_id(game: &GameState, args: &mut Args) -> Result<Arc<str>, String> {
    let id = normalize_id(&word(args)?);
    game.registries
        .entity_types
        .iter()
        .find(|i| ***i == *id)
        .cloned()
        .ok_or_else(|| "no entity found by that id".into())
}

pub fn complete_entity_type(game: &GameState, args: &mut Args) -> Vec<String> {
    let Ok(s) = args.get_string() else { return Vec::new() };
    Completions::from(game.registries.entity_types.iter().map(|i| i.to_string()))
        .filter_prefix_namespaced(&s)
        .sort_alphabetically()
        .build()
}

/// `NearbyPlayer`: a player the client can see, by name; `None` if there is none by that name.
pub fn nearby_player(game: &GameState, args: &mut Args) -> Result<Option<EntityInfo>, String> {
    let name = word(args)?;
    Ok(game
        .entities
        .iter()
        .find(|e| e.is_player && e.name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(&name)))
        .cloned())
}

pub fn complete_nearby_player(game: &GameState, args: &mut Args) -> Vec<String> {
    let Ok(s) = args.get_string() else { return Vec::new() };
    Completions::from(
        game.entities
            .iter()
            .filter(|e| e.is_player)
            .filter_map(|e| e.name.as_deref().map(str::to_string)),
    )
    .filter_prefix(&s)
    .sort_alphabetically()
    .build()
}

/// `ForDirection`.
pub fn direction(args: &mut Args) -> Result<Direction, String> {
    let s = word(args)?;
    Direction::from_name(&s).ok_or_else(|| format!("No enum constant {}", s.to_uppercase()))
}

pub fn complete_direction(args: &mut Args) -> Vec<String> {
    let Ok(s) = args.get_string() else { return Vec::new() };
    Completions::from(Direction::ALL.iter().map(|d| d.name()))
        .filter_prefix(&s)
        .build()
}

/// Waypoints newest first (`ForWaypoints.getWaypoints`).
pub fn waypoints_newest_first<'a>(all: impl Iterator<Item = &'a Waypoint>) -> Vec<Waypoint> {
    let mut v: Vec<Waypoint> = all.cloned().collect();
    v.sort_by_key(|w| std::cmp::Reverse(w.creation_timestamp));
    v
}

/// `ForWaypoints`: the waypoints with a tag, or else those with a name (ignoring case), newest first.
pub fn for_waypoints(waypoints: &WaypointCollection, args: &mut Args) -> Result<Vec<Waypoint>, String> {
    let input = word(args)?;
    Ok(match Tag::by_name(&input) {
        Some(tag) => waypoints_newest_first(waypoints.by_tag(tag)),
        None => waypoints_newest_first(waypoints.all().iter().filter(|w| w.name.eq_ignore_ascii_case(&input))),
    })
}

/// `ForWaypoints.tabComplete`: the tags, then the waypoints' names.
pub fn complete_waypoints(waypoints: Option<&WaypointCollection>, args: &mut Args) -> Vec<String> {
    let Ok(s) = args.get_string() else { return Vec::new() };
    let names: Vec<String> = waypoints
        .map(|w| {
            waypoints_newest_first(w.all().iter())
                .into_iter()
                .map(|w| w.name)
                .filter(|n| !n.is_empty())
                .collect()
        })
        .unwrap_or_default();
    Completions::from(names)
        .sort_alphabetically()
        .prepend(Tag::all_names())
        .filter_prefix(&s)
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coordinates() {
        assert_eq!(parse_relative_coordinate("~", 10.0), Some(10.0));
        assert_eq!(parse_relative_coordinate("~-3", 10.0), Some(7.0));
        assert_eq!(parse_relative_coordinate("5", 10.0), Some(5.0));
        assert_eq!(parse_relative_coordinate("1.5k", 0.0), Some(1500.0));
        assert_eq!(parse_relative_coordinate("~2M", 1.0), Some(2_000_001.0));
        assert_eq!(parse_relative_coordinate("", 10.0), Some(0.0));
        assert_eq!(parse_relative_coordinate("k", 0.0), None);
        assert_eq!(parse_relative_coordinate("x", 0.0), None);
    }

    #[test]
    fn goals_from_words() {
        let origin = BlockPos::new(10, 64, -5);
        let g = relative_goal(&mut Args::parse_line("1 2 3"), origin).unwrap();
        assert_eq!(g.to_string(), GoalBlock::new(1, 2, 3).to_string());
        let g = relative_goal(&mut Args::parse_line("~5 ~"), origin).unwrap();
        assert_eq!(g.to_string(), GoalXZ::new(15, -5).to_string());
        let g = relative_goal(&mut Args::parse_line("~-1"), origin).unwrap();
        assert_eq!(g.to_string(), GoalYLevel::new(63).to_string());
        let g = relative_goal(&mut Args::parse_line(""), origin).unwrap();
        assert_eq!(g.to_string(), GoalBlock::at(origin).to_string());
    }
}
