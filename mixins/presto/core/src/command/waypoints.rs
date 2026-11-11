//! `WaypointsCommand`: `#wp list/save/info/delete/restore/clear/goal/goto`.

use super::args::Args;
use super::datatypes::{complete_relative_block_pos, complete_waypoints, for_waypoints, relative_block_pos, waypoints_newest_first};
use super::defaults::pos_string;
use super::helpers::{Completions, clickable, paginate, span, suggesting};
use super::set::lines;
use super::{Command, CommandError, Ctx};
use crate::cache::waypoints::{Tag, Waypoint, WaypointCollection};
use crate::pathing::goals::{GoalBlock, GoalRef};
use crate::player::{Span, chat_color};
use crate::utils::output::force_command_prefix;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    List,
    Clear,
    Save,
    Info,
    Delete,
    Restore,
    Goal,
    Goto,
}

impl Action {
    const ALL: [Action; 8] = [
        Action::List,
        Action::Clear,
        Action::Save,
        Action::Info,
        Action::Delete,
        Action::Restore,
        Action::Goal,
        Action::Goto,
    ];

    fn names(self) -> &'static [&'static str] {
        match self {
            Action::List => &["list", "get", "l"],
            Action::Clear => &["clear", "c"],
            Action::Save => &["save", "s"],
            Action::Info => &["info", "show", "i"],
            Action::Delete => &["delete", "d"],
            Action::Restore => &["restore"],
            Action::Goal => &["goal", "g"],
            Action::Goto => &["goto"],
        }
    }

    fn by_name(name: &str) -> Option<Action> {
        Self::ALL
            .into_iter()
            .find(|a| a.names().iter().any(|n| n.eq_ignore_ascii_case(name)))
    }

    fn all_names() -> Vec<&'static str> {
        Self::ALL.iter().flat_map(|a| a.names().iter().copied()).collect()
    }
}

fn tag_upper(tag: Tag) -> String {
    tag.name().to_uppercase()
}

/// How a waypoint shows in a list: `TAG name @ date`, click to run `action` on it.
fn waypoint_spans(label: &str, w: &Waypoint, action: Action) -> Vec<Span> {
    let command = format!(
        "{}{label} {} {} @ {}",
        force_command_prefix(),
        action.names()[0],
        w.tag.name(),
        w.creation_timestamp
    );
    let hover = "Click to select";
    let name = if w.name.is_empty() { "<empty>" } else { w.name.as_str() };
    let name_color = if w.name.is_empty() {
        chat_color::DARK_GRAY
    } else {
        chat_color::GRAY
    };
    vec![
        clickable(format!("{} ", tag_upper(w.tag)), chat_color::GRAY, command.clone(), hover),
        clickable(name, name_color, command.clone(), hover),
        clickable(format!(" @ {}", format_date(w.creation_timestamp)), chat_color::DARK_GRAY, command, hover),
    ]
}

/// `new Date(ms).toString()`, in UTC: `Tue Nov 12 14:03:07 UTC 2026`.
fn format_date(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (h, m, s) = (rem / 3600, (rem / 60) % 60, rem % 60);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    const DAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    format!(
        "{} {} {day:02} {h:02}:{m:02}:{s:02} UTC {year}",
        DAYS[days.rem_euclid(7) as usize],
        MONTHS[(month - 1) as usize]
    )
}

pub struct Waypoints;

fn collection<'a>(c: &'a mut Ctx) -> Result<&'a mut WaypointCollection, CommandError> {
    c.b.bot
        .world
        .current_mut()
        .map(|w| &mut w.waypoints)
        .ok_or_else(|| CommandError::InvalidState("You need to be in a world for that".into()))
}

impl Command for Waypoints {
    fn names(&self) -> &[&'static str] {
        &["waypoints", "waypoint", "wp"]
    }

    fn execute(&self, c: &mut Ctx, label: &str, args: &mut Args) -> Result<(), CommandError> {
        let action = if args.has_any() {
            let s = args.get_string()?;
            Action::by_name(&s).ok_or_else(|| CommandError::invalid_type(&args.consumed(), "an action"))?
        } else {
            Action::List
        };
        let shown_action = if action == Action::List { Action::Info } else { action };
        let label = label.to_string();
        match action {
            Action::List => {
                let tag = if args.has_any() {
                    Tag::by_name(args.peek_string(0)?)
                } else {
                    None
                };
                if tag.is_some() {
                    args.get()?;
                }
                let all = collection(c)?;
                let waypoints = match tag {
                    Some(t) => waypoints_newest_first(all.by_tag(t)),
                    None => waypoints_newest_first(all.all().iter()),
                };
                if waypoints.is_empty() {
                    args.require_max(0)?;
                    return Err(CommandError::InvalidState(
                        if tag.is_some() {
                            "No waypoints found by that tag"
                        } else {
                            "No waypoints found"
                        }
                        .into(),
                    ));
                }
                args.require_max(1)?;
                let header = match tag {
                    Some(t) => format!("All waypoints by tag {}:", tag_upper(t)),
                    None => "All waypoints:".into(),
                };
                let prefix = format!(
                    "{}{label} {}{}",
                    force_command_prefix(),
                    action.names()[0],
                    tag.map(|t| format!(" {}", t.name())).unwrap_or_default()
                );
                paginate(
                    c,
                    args,
                    &waypoints,
                    |c| c.log(&header),
                    |w| waypoint_spans(&label, w, shown_action),
                    Some(prefix),
                )
            }
            Action::Save => {
                let tag = if args.has_any() {
                    Tag::by_name(args.peek_string(0)?)
                } else {
                    None
                };
                let tag = match tag {
                    Some(t) => {
                        args.get()?;
                        t
                    }
                    None => Tag::User,
                };
                let name = if args.has_exactly(1) || args.has_exactly(4) {
                    args.get_string()?
                } else {
                    String::new()
                };
                let feet = c.pc()?.player_feet();
                let pos = if args.has_any() {
                    args.datatype("RelativeBlockPos", |a| relative_block_pos(a, feet))?
                } else {
                    feet
                };
                args.require_max(0)?;
                let waypoint = Waypoint::new(name, tag, pos);
                collection(c)?.add(waypoint.clone());
                let mut spans = vec![span("Waypoint added: ", chat_color::GRAY)];
                spans.extend(waypoint_spans(&label, &waypoint, Action::Info));
                c.log_spans(spans);
                Ok(())
            }
            Action::Clear => {
                args.require_max(1)?;
                let name = args.get_string()?;
                let tag = Tag::by_name(&name)
                    .ok_or_else(|| CommandError::InvalidState(format!("Invalid tag, \"{name}\"")))?;
                let all = collection(c)?;
                let waypoints = waypoints_newest_first(all.by_tag(tag));
                for w in &waypoints {
                    all.remove(w);
                }
                let stamps: Vec<String> = waypoints.iter().map(|w| w.creation_timestamp.to_string()).collect();
                let n = waypoints.len();
                c.b.deleted_waypoints().extend(waypoints);
                c.log_spans(vec![clickable(
                    format!("Cleared {n} waypoints, click to restore them"),
                    chat_color::GRAY,
                    format!("{}{label} restore @ {}", force_command_prefix(), stamps.join(" ")),
                    "",
                )]);
                Ok(())
            }
            Action::Restore => {
                let deleted = c.b.deleted_waypoints().clone();
                let mut restore: Vec<Waypoint> = Vec::new();
                if args.peek_string(0)? == "@" {
                    args.get()?;
                    // Clearing an empty tag leaves nothing to restore, so no words after @ is fine.
                    while args.has_any() {
                        let stamp = args.get_as::<i64>()?;
                        if let Some(w) = deleted.iter().find(|w| w.creation_timestamp == stamp) {
                            restore.push(w.clone());
                        }
                    }
                } else {
                    args.require_exactly(1)?;
                    let size = deleted.len();
                    let amount = (args.get_as::<i32>()?.max(0) as usize).min(size);
                    restore = deleted[size - amount..].to_vec();
                }
                let all = collection(c)?;
                for w in &restore {
                    all.add(w.clone());
                }
                c.b.deleted_waypoints().retain(|w| !restore.contains(w));
                c.log(&format!("Restored {} waypoints", restore.len()));
                Ok(())
            }
            Action::Info | Action::Delete | Action::Goal | Action::Goto => {
                let waypoints = {
                    let all = collection(c)?;
                    args.datatype("ForWaypoints", |a| for_waypoints(all, a))?
                };
                let mut waypoint = None;
                if args.has_any() && args.peek_string(0)? == "@" {
                    args.require_exactly(2)?;
                    args.get()?;
                    let stamp = args.get_as::<i64>()?;
                    waypoint = waypoints.iter().find(|w| w.creation_timestamp == stamp).cloned();
                    if waypoint.is_none() {
                        return Err(CommandError::InvalidState(
                            "Timestamp was specified but no waypoint was found".into(),
                        ));
                    }
                } else {
                    match waypoints.len() {
                        0 => return Err(CommandError::InvalidState("No waypoints found".into())),
                        1 => waypoint = Some(waypoints[0].clone()),
                        _ => {}
                    }
                }
                let Some(waypoint) = waypoint else {
                    args.require_max(1)?;
                    let prefix = format!(
                        "{}{label} {} {}",
                        force_command_prefix(),
                        action.names()[0],
                        args.consumed_string()
                    );
                    return paginate(
                        c,
                        args,
                        &waypoints,
                        |c| c.log("Multiple waypoints were found:"),
                        |w| waypoint_spans(&label, w, shown_action),
                        Some(prefix),
                    );
                };
                let fp = force_command_prefix();
                let (tag, stamp) = (waypoint.tag.name(), waypoint.creation_timestamp);
                match action {
                    Action::Info => {
                        c.log_spans(waypoint_spans(&label, &waypoint, shown_action));
                        c.log(&format!("Position: {}", pos_string(waypoint.location)));
                        c.log_spans(vec![clickable(
                            "Click to delete this waypoint",
                            chat_color::GRAY,
                            format!("{fp}{label} delete {tag} @ {stamp}"),
                            "",
                        )]);
                        c.log_spans(vec![clickable(
                            "Click to set goal to this waypoint",
                            chat_color::GRAY,
                            format!("{fp}{label} goal {tag} @ {stamp}"),
                            "",
                        )]);
                        let l = waypoint.location;
                        // The user's own prefix: they run this one themselves.
                        let prefix = c.settings().prefix.clone();
                        c.log_spans(vec![suggesting(
                            "Click to show a command to recreate this waypoint",
                            chat_color::GRAY,
                            format!("{prefix}{label} save {tag} {} {} {} {}", waypoint.name, l.x, l.y, l.z),
                            "",
                        )]);
                        c.log_spans(vec![clickable(
                            "Click to return to the waypoints list",
                            chat_color::GRAY,
                            format!("{fp}{label} list"),
                            "",
                        )]);
                    }
                    Action::Delete => {
                        collection(c)?.remove(&waypoint);
                        c.b.deleted_waypoints().push(waypoint.clone());
                        c.log_spans(vec![clickable(
                            "That waypoint has successfully been deleted, click to restore it",
                            chat_color::GRAY,
                            format!("{fp}{label} restore @ {stamp}"),
                            "",
                        )]);
                    }
                    Action::Goal => {
                        let goal: GoalRef = Arc::new(GoalBlock::at(waypoint.location));
                        c.b.procs.custom_goal.set_goal(Some(goal.clone()));
                        c.log(&format!("Goal: {goal}"));
                    }
                    _ => {
                        let goal: GoalRef = Arc::new(GoalBlock::at(waypoint.location));
                        c.b.procs.custom_goal.set_goal_and_path(goal.clone());
                        c.log(&format!("Going to: {goal}"));
                    }
                }
                Ok(())
            }
        }
    }

    fn tab_complete(&self, c: &mut Ctx, _label: &str, args: &mut Args) -> Result<Vec<String>, CommandError> {
        if !args.has_any() {
            return Ok(Vec::new());
        }
        if args.has_exactly(1) {
            return Ok(Completions::from(Action::all_names())
                .sort_alphabetically()
                .filter_prefix(&args.get_string()?)
                .build());
        }
        let action = Action::by_name(&args.get_string()?);
        if args.has_exactly(1) {
            return Ok(match action {
                Some(Action::List | Action::Save | Action::Clear) => Completions::from(Tag::all_names())
                    .sort_alphabetically()
                    .filter_prefix(&args.get_string()?)
                    .build(),
                Some(Action::Restore) => Vec::new(),
                _ => complete_waypoints(c.b.bot.world.current().map(|w| &w.waypoints), args),
            });
        }
        if args.has(3) && action == Some(Action::Save) {
            args.get()?;
            args.get()?;
            return Ok(complete_relative_block_pos(args));
        }
        Ok(Vec::new())
    }

    fn short_desc(&self) -> String {
        "Manage waypoints".into()
    }

    fn long_desc(&self) -> Vec<String> {
        lines(&[
            "The waypoint command allows you to manage Presto's waypoints.",
            "",
            "Waypoints can be used to mark positions for later. Waypoints are each given a tag and an optional name.",
            "",
            "Note that the info, delete, and goal commands let you specify a waypoint by tag. If there is more than one waypoint with a certain tag, then they will let you select which waypoint you mean.",
            "",
            "Missing arguments for the save command use the USER tag, creating an unnamed waypoint and your current position as defaults.",
            "",
            "Usage:",
            "> wp [l/list] - List all waypoints.",
            "> wp <l/list> <tag> - List all waypoints by tag.",
            "> wp <s/save> - Save an unnamed USER waypoint at your current position",
            "> wp <s/save> [tag] [name] [pos] - Save a waypoint with the specified tag, name and position.",
            "> wp <i/info/show> <tag/name> - Show info on a waypoint by tag or name.",
            "> wp <d/delete> <tag/name> - Delete a waypoint by tag or name.",
            "> wp <restore> <n> - Restore the last n deleted waypoints.",
            "> wp <c/clear> <tag> - Delete all waypoints with the specified tag.",
            "> wp <g/goal> <tag/name> - Set a goal to a waypoint by tag or name.",
            "> wp <goto> <tag/name> - Set a goal to a waypoint by tag or name and start pathing.",
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_like_java() {
        assert_eq!(format_date(0), "Thu Jan 01 00:00:00 UTC 1970");
        assert_eq!(format_date(1_731_420_187_000), "Tue Nov 12 14:03:07 UTC 2024");
    }
}
