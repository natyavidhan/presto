//! Presto's default commands (`baritone.command.defaults`), but `#set`, `#help` and `#waypoints`.

use super::args::Args;
use super::datatypes::*;
use super::helpers::{Completions, clickable};
use super::set::lines;
use super::{Command, CommandError, Ctx};
use crate::math::{BlockPos, Direction};
use crate::pathing::goals::{GoalAxis, GoalBlock, GoalInverted, GoalRef, GoalStrictDirection, GoalXZ};
use crate::player::chat_color;
use crate::process::follow::FollowFilter;
use crate::settings::maybe_censor;
use crate::utils::block_optional_meta::BlockOptionalMetaLookup;
use crate::utils::output::force_command_prefix;
use std::sync::Arc;

/// `BetterBlockPos.toString`.
pub fn pos_string(p: BlockPos) -> String {
    format!("BetterBlockPos{{x={},y={},z={}}}", maybe_censor(p.x), maybe_censor(p.y), maybe_censor(p.z))
}

/// A command with no arguments and nothing to complete: just names, descriptions and what it does.
macro_rules! simple_command {
    ($ty:ident, [$($name:literal),+], $short:literal, [$($long:literal),* $(,)?], |$c:ident, $label:ident, $args:ident| $body:block) => {
        pub struct $ty;
        impl Command for $ty {
            fn names(&self) -> &[&'static str] {
                &[$($name),+]
            }
            fn short_desc(&self) -> String {
                $short.into()
            }
            fn long_desc(&self) -> Vec<String> {
                lines(&[$($long),*])
            }
            #[allow(unused_variables)]
            fn execute(&self, $c: &mut Ctx, $label: &str, $args: &mut Args) -> Result<(), CommandError> $body
        }
    };
}

pub struct GoalCommand;

impl Command for GoalCommand {
    fn names(&self) -> &[&'static str] {
        &["goal"]
    }
    fn execute(&self, c: &mut Ctx, _label: &str, args: &mut Args) -> Result<(), CommandError> {
        if args.has_any() && ["reset", "clear", "none"].contains(&args.peek_string(0)?) {
            args.require_max(1)?;
            if c.b.procs.custom_goal.goal().is_some() {
                c.b.procs.custom_goal.set_goal(None);
                c.log("Cleared goal");
            } else {
                c.log("There was no goal to clear");
            }
            return Ok(());
        }
        args.require_max(3)?;
        let origin = c.pc()?.player_feet();
        let goal = args.datatype("RelativeGoal", |a| relative_goal(a, origin))?;
        c.b.procs.custom_goal.set_goal(Some(goal.clone()));
        c.log(&format!("Goal: {goal}"));
        Ok(())
    }
    fn tab_complete(&self, _c: &mut Ctx, _label: &str, args: &mut Args) -> Result<Vec<String>, CommandError> {
        let mut helper = Completions::new();
        if args.has_exactly(1) {
            helper = helper.append(["reset", "clear", "none", "~"]);
        } else if args.has_at_most(3) {
            while args.has(2) {
                if args.peek_datatype_or_none(|a| relative_coordinate(a, 0.0)).is_none() {
                    break;
                }
                args.get()?;
                if !args.has(2) {
                    helper = helper.append(["~"]);
                }
            }
        }
        Ok(helper.filter_prefix(&args.get_string()?).build())
    }
    fn short_desc(&self) -> String {
        "Set or clear the goal".into()
    }
    fn long_desc(&self) -> Vec<String> {
        lines(&[
            "The goal command allows you to set or clear Presto's goal.",
            "",
            "Wherever a coordinate is expected, you can use ~ just like in regular Minecraft commands. Or, you can just use regular numbers.",
            "",
            "Usage:",
            "> goal - Set the goal to your current position",
            "> goal <reset/clear/none> - Erase the goal",
            "> goal <y> - Set the goal to a Y level",
            "> goal <x> <z> - Set the goal to an X,Z position",
            "> goal <x> <y> <z> - Set the goal to an X,Y,Z position",
        ])
    }
}

pub struct GotoCommand;

impl Command for GotoCommand {
    fn names(&self) -> &[&'static str] {
        &["goto"]
    }
    fn execute(&self, c: &mut Ctx, _label: &str, args: &mut Args) -> Result<(), CommandError> {
        // A number first: coordinates. (There's no reason to go where you are, so no words is a block.)
        if args.peek_datatype_or_none(|a| relative_coordinate(a, 0.0)).is_some() {
            args.require_max(3)?;
            let origin = c.pc()?.player_feet();
            let goal = args.datatype("RelativeGoal", |a| relative_goal(a, origin))?;
            c.log(&format!("Going to: {goal}"));
            c.b.procs.custom_goal.set_goal_and_path(goal);
            return Ok(());
        }
        args.require_max(1)?;
        let table = c.pc()?.world.table.clone();
        let destination = args.datatype("ForBlockOptionalMeta", |a| block_optional_meta(&table, a))?;
        c.with_processes(|p, pc| p.get_to_block.get_to_block(pc, destination))?;
        Ok(())
    }
    fn tab_complete(&self, c: &mut Ctx, _label: &str, args: &mut Args) -> Result<Vec<String>, CommandError> {
        args.require_max(1)?;
        let Ok(pc) = c.pc() else { return Ok(Vec::new()) };
        Ok(complete_block_optional_meta(&pc.world.table, args))
    }
    fn short_desc(&self) -> String {
        "Go to a coordinate or block".into()
    }
    fn long_desc(&self) -> Vec<String> {
        lines(&[
            "The goto command tells Presto to head towards a given goal or block.",
            "",
            "Wherever a coordinate is expected, you can use ~ just like in regular Minecraft commands. Or, you can just use regular numbers.",
            "",
            "Usage:",
            "> goto <block> - Go to a block, wherever it is in the world",
            "> goto <y> - Go to a Y level",
            "> goto <x> <z> - Go to an X,Z position",
            "> goto <x> <y> <z> - Go to an X,Y,Z position",
        ])
    }
}

simple_command!(PathCommand, ["path"], "Start heading towards the goal", [
    "The path command tells Presto to head towards the current goal.",
    "",
    "Usage:",
    "> path - Start the pathing.",
], |c, label, args| {
    args.require_max(0)?;
    if c.b.procs.custom_goal.goal().is_none() {
        c.log("No goal set");
        return Ok(());
    }
    if let Some(pc) = c.pc {
        c.b.repack(pc.game, 40);
    }
    c.b.procs.custom_goal.path();
    c.log("Now pathing");
    Ok(())
});

simple_command!(ProcCommand, ["proc"], "View process state information", [
    "The proc command provides miscellaneous information about the process currently controlling Presto.",
    "",
    "You are not expected to understand this if you aren't familiar with how Presto works.",
    "",
    "Usage:",
    "> proc - View process information, if present",
], |c, label, args| {
    args.require_max(0)?;
    let id = c
        .b
        .control
        .most_recent_in_control()
        .ok_or_else(|| CommandError::InvalidState("No process in control".into()))?;
    let command = c
        .b
        .control
        .most_recent_command()
        .map(|cmd| format!("{cmd:?}"))
        .unwrap_or_else(|| "None".into());
    let p = c.b.procs.get_mut(id);
    let msg = format!(
        "Class: {:?}\nPriority: {:.6}\nTemporary: {}\nDisplay name: {}\nLast command: {command}",
        id,
        p.priority(),
        p.is_temporary(),
        p.display_name()
    );
    c.log(&msg);
    Ok(())
});

simple_command!(EtaCommand, ["eta"], "View the current ETA", [
    "The ETA command provides information about the estimated time until the next segment.",
    "and the goal",
    "",
    "Be aware that the ETA to your goal is really unprecise",
    "",
    "Usage:",
    "> eta - View ETA, if present",
], |c, label, args| {
    args.require_max(0)?;
    if c.b.control.most_recent_in_control().is_none() {
        return Err(CommandError::InvalidState("No process in control".into()));
    }
    let feet = c.pc()?.player_feet();
    let segment = c.b.bot.pathing.ticks_remaining_in_segment(true).unwrap_or(f64::NAN);
    let goal = c.b.bot.pathing.estimated_ticks_to_goal(feet).unwrap_or(f64::NAN);
    // TPS is taken to be 20.
    c.log(&format!(
        "Next segment: {:.1}s ({:.0} ticks)\nGoal: {:.1}s ({:.0} ticks)",
        segment / 20.0,
        segment,
        goal / 20.0,
        goal
    ));
    Ok(())
});

simple_command!(VersionCommand, ["version"], "View the Presto version", [
    "The version command prints the version of Presto you're currently running.",
    "",
    "Usage:",
    "> version - View version information, if present",
], |c, label, args| {
    args.require_max(0)?;
    c.log(&format!(
        "You are running Presto v{}, Baritone ported to Horizon",
        env!("CARGO_PKG_VERSION")
    ));
    Ok(())
});

simple_command!(RepackCommand, ["repack", "rescan"], "Re-cache chunks", [
    "Repack chunks around you. This basically re-caches them.",
    "",
    "Usage:",
    "> repack - Repack chunks.",
], |c, label, args| {
    args.require_max(0)?;
    let game = c.pc()?.game;
    let n = c.b.repack(game, 40);
    c.log(&format!("Queued {n} chunks for repacking"));
    Ok(())
});

simple_command!(ComeCommand, ["come"], "Start heading towards your camera", [
    "The come command tells Presto to head towards your camera.",
    "",
    "This can be useful in hacked clients where freecam doesn't move your player position.",
    "",
    "Usage:",
    "> come",
], |c, label, args| {
    args.require_max(0)?;
    let pc = c.pc()?;
    let viewer = pc.game.camera.unwrap_or(pc.player.position);
    c.b.procs.custom_goal.set_goal_and_path(Arc::new(GoalBlock::at(BlockPos::of_vec(viewer))));
    c.log("Coming");
    Ok(())
});

simple_command!(AxisCommand, ["axis", "highway"], "Set a goal to the axes", [
    "The axis command sets a goal that tells Presto to head towards the nearest axis. That is, X=0 or Z=0.",
    "",
    "Usage:",
    "> axis",
], |c, label, args| {
    args.require_max(0)?;
    let goal: GoalRef = Arc::new(GoalAxis);
    c.b.procs.custom_goal.set_goal(Some(goal.clone()));
    c.log(&format!("Goal: {goal}"));
    Ok(())
});

simple_command!(ForceCancelCommand, ["forcecancel"], "Force cancel", [
    "Like cancel, but more forceful.",
    "",
    "Usage:",
    "> forcecancel",
], |c, label, args| {
    args.require_max(0)?;
    c.b.cancel_everything();
    let b = &mut *c.b;
    b.bot.pathing.force_cancel(&mut b.bot.input);
    c.log("ok force canceled");
    Ok(())
});

simple_command!(GcCommand, ["gc"], "Call System.gc()", [
    "Calls System.gc().",
    "",
    "Usage:",
    "> gc",
], |c, label, args| {
    args.require_max(0)?;
    // Rust frees memory as soon as it is unused; the nearest thing is dropping the cache's far regions.
    if let Some(w) = c.b.bot.world.current() {
        w.cache.prune_far_regions();
    }
    c.log("ok, nothing to collect (no garbage collector here); far cached regions dropped from memory");
    Ok(())
});

simple_command!(InvertCommand, ["invert"], "Run away from the current goal", [
    "The invert command tells Presto to head away from the current goal rather than towards it.",
    "",
    "Usage:",
    "> invert - Invert the current goal.",
], |c, label, args| {
    args.require_max(0)?;
    let goal = c
        .b
        .procs
        .custom_goal
        .goal()
        .cloned()
        .ok_or_else(|| CommandError::InvalidState("No goal".into()))?;
    let goal: GoalRef = match goal.as_any().downcast_ref::<GoalInverted>() {
        Some(inv) => inv.origin.clone(),
        None => Arc::new(GoalInverted { origin: goal }),
    };
    c.b.procs.custom_goal.set_goal_and_path(goal.clone());
    c.log(&format!("Goal: {goal}"));
    Ok(())
});

pub struct TunnelCommand;

impl Command for TunnelCommand {
    fn names(&self) -> &[&'static str] {
        &["tunnel"]
    }
    fn execute(&self, c: &mut Ctx, _label: &str, args: &mut Args) -> Result<(), CommandError> {
        args.require_max(3)?;
        let pc = c.pc()?;
        let feet = pc.player_feet();
        let facing = Direction::from_yaw(pc.player.yaw as f64);
        if args.has_exactly(3) {
            let int = |i: usize| -> Result<i32, CommandError> { args.peek_as::<i32>(i) };
            let (mut height, mut width, depth) = (int(0)?, int(1)?, int(2)?);
            if width < 1 || height < 2 || depth < 1 || height > pc.world.bsi.dimension.max_y() - 1 {
                c.log("Width and depth must at least be 1 block; Height must at least be 2 blocks, and cannot be greater than the build limit.");
                return Ok(());
            }
            height -= 1;
            width -= 1;
            let add = if width % 2 == 0 { 0 } else { 1 };
            let (x, y, z) = (feet.x, feet.y, feet.z);
            let (c1, c2) = match facing {
                Direction::East => (
                    BlockPos::new(x, y, z - width / 2),
                    BlockPos::new(x + depth, y + height, z + width / 2 + add),
                ),
                Direction::West => (
                    BlockPos::new(x, y, z + width / 2 + add),
                    BlockPos::new(x - depth, y + height, z - width / 2),
                ),
                Direction::North => (
                    BlockPos::new(x - width / 2, y, z),
                    BlockPos::new(x + width / 2 + add, y + height, z - depth),
                ),
                _ => (
                    BlockPos::new(x + width / 2 + add, y, z),
                    BlockPos::new(x - width / 2, y + height, z + depth),
                ),
            };
            c.log(&format!(
                "Creating a tunnel {} block(s) high, {} block(s) wide, and {depth} block(s) deep",
                height + 1,
                width + 1
            ));
            let b = &mut *c.b;
            b.procs.builder.clear_area(&mut b.bot, c1, c2);
        } else {
            let goal: GoalRef = Arc::new(GoalStrictDirection::new(feet, facing));
            c.b.procs.custom_goal.set_goal_and_path(goal.clone());
            c.log(&format!("Goal: {goal}"));
        }
        Ok(())
    }
    fn short_desc(&self) -> String {
        "Set a goal to tunnel in your current direction".into()
    }
    fn long_desc(&self) -> Vec<String> {
        lines(&[
            "The tunnel command sets a goal that tells Presto to mine completely straight in the direction that you're facing.",
            "",
            "Usage:",
            "> tunnel - No arguments, mines in a 1x2 radius.",
            "> tunnel <height> <width> <depth> - Tunnels in a user defined height, width and depth.",
        ])
    }
}

simple_command!(RenderCommand, ["render"], "Fix glitched chunks", [
    "The render command fixes glitched chunk rendering without having to reload all of them.",
    "",
    "Usage:",
    "> render",
], |c, label, args| {
    args.require_max(0)?;
    c.b.bot.out.rerender_chunks = true;
    c.log("Done");
    Ok(())
});

/// `FollowCommand.FollowGroup` and `FollowList`.
const FOLLOW_GROUPS: [&str; 2] = ["entities", "players"];
const FOLLOW_LISTS: [&str; 2] = ["entity", "player"];

pub struct FollowCommand;

impl Command for FollowCommand {
    fn names(&self) -> &[&'static str] {
        &["follow"]
    }
    fn execute(&self, c: &mut Ctx, _label: &str, args: &mut Args) -> Result<(), CommandError> {
        args.require_min(1)?;
        let game = c.pc()?.game;
        if args.has_exactly(1) {
            let arg = args.get()?;
            let filter = match arg.value.to_lowercase().as_str() {
                "entities" => FollowFilter::Entities,
                "players" => FollowFilter::Players,
                _ => return Err(CommandError::invalid_type(&arg, "FollowGroup")),
            };
            c.b.procs.follow.follow(filter);
            c.log(&format!("Following all {}", arg.value.to_lowercase()));
            return Ok(());
        }
        args.require_min(2)?;
        let list = args.get()?;
        match list.value.to_lowercase().as_str() {
            "entity" => {
                let mut kinds: Vec<String> = Vec::new();
                while args.has_any() {
                    let kind = args.datatype("EntityClassById", |a| entity_type_by_id(game, a))?;
                    kinds.push(kind.to_string());
                }
                c.b.procs.follow.follow(FollowFilter::EntityTypes(kinds.clone()));
                c.log("Following these types of entities:");
                for k in kinds {
                    c.log(&k);
                }
            }
            "player" => {
                let mut players = Vec::new();
                while args.has_any() {
                    if let Some(p) = args.datatype("NearbyPlayer", |a| nearby_player(game, a))? {
                        players.push(p);
                    }
                }
                if players.is_empty() {
                    return Err(CommandError::Message("No valid entities in range!".into()));
                }
                let names: Vec<String> = players
                    .iter()
                    .filter_map(|p| p.name.as_deref().map(str::to_string))
                    .collect();
                c.b.procs.follow.follow(FollowFilter::PlayerNames(names));
                c.log("Following these entities:");
                for p in players {
                    c.log(&format!(
                        "Player['{}'/{}, x={:.2}, y={:.2}, z={:.2}]",
                        p.name.as_deref().unwrap_or(""),
                        p.id,
                        p.position.x,
                        p.position.y,
                        p.position.z
                    ));
                }
            }
            _ => return Err(CommandError::invalid_type(&list, "FollowList")),
        }
        Ok(())
    }
    fn tab_complete(&self, c: &mut Ctx, _label: &str, args: &mut Args) -> Result<Vec<String>, CommandError> {
        if args.has_exactly(1) {
            return Ok(Completions::from(FOLLOW_GROUPS)
                .append(FOLLOW_LISTS)
                .filter_prefix(&args.get_string()?)
                .build());
        }
        let Ok(pc) = c.pc() else { return Ok(Vec::new()) };
        let game = pc.game;
        let players = match args.get_string()?.to_lowercase().as_str() {
            "entity" => false,
            "player" => true,
            _ => return Ok(Vec::new()),
        };
        while args.has(2) {
            let ok = if players {
                args.peek_datatype_or_none(|a| nearby_player(game, a)).flatten().is_some()
            } else {
                args.peek_datatype_or_none(|a| entity_type_by_id(game, a)).is_some()
            };
            if !ok {
                return Ok(Vec::new());
            }
            args.get()?;
        }
        Ok(if players {
            complete_nearby_player(game, args)
        } else {
            complete_entity_type(game, args)
        })
    }
    fn short_desc(&self) -> String {
        "Follow entity things".into()
    }
    fn long_desc(&self) -> Vec<String> {
        lines(&[
            "The follow command tells Presto to follow certain kinds of entities.",
            "",
            "Usage:",
            "> follow entities - Follows all entities.",
            "> follow entity <entity1> <entity2> <...> - Follow certain entities (for example 'skeleton', 'horse' etc.)",
            "> follow players - Follow players",
            "> follow player <username1> <username2> <...> - Follow certain players",
        ])
    }
}

pub struct PickupCommand;

impl Command for PickupCommand {
    fn names(&self) -> &[&'static str] {
        &["pickup"]
    }
    fn execute(&self, c: &mut Ctx, _label: &str, args: &mut Args) -> Result<(), CommandError> {
        let game = c.pc()?.game;
        let mut items: Vec<String> = Vec::new();
        while args.has_any() {
            let item = args.datatype("ItemById", |a| item_by_id(game, a))?;
            if !items.iter().any(|i| **i == *item) {
                items.push(item.to_string());
            }
        }
        c.b.procs.follow.pickup(items.clone());
        if items.is_empty() {
            c.log("Picking up all items");
        } else {
            c.log("Picking up these items:");
            for i in items {
                c.log(&i);
            }
        }
        Ok(())
    }
    fn tab_complete(&self, c: &mut Ctx, _label: &str, args: &mut Args) -> Result<Vec<String>, CommandError> {
        let Ok(pc) = c.pc() else { return Ok(Vec::new()) };
        while args.has(2) {
            if args.peek_datatype_or_none(|a| item_by_id(pc.game, a)).is_none() {
                return Ok(Vec::new());
            }
            args.get()?;
        }
        Ok(complete_item_id(pc.game, args))
    }
    fn short_desc(&self) -> String {
        "Pickup items".into()
    }
    fn long_desc(&self) -> Vec<String> {
        lines(&[
            "Usage:",
            "> pickup - Pickup anything",
            "> pickup <item1> <item2> <...> - Pickup certain items",
        ])
    }
}

simple_command!(ReloadAllCommand, ["reloadall"], "Reloads Presto's cache for this world", [
    "The reloadall command reloads Presto's world cache.",
    "",
    "Usage:",
    "> reloadall",
], |c, label, args| {
    args.require_max(0)?;
    let w = c
        .b
        .bot
        .world
        .current()
        .ok_or_else(|| CommandError::InvalidState("You need to be in a world for that".into()))?;
    w.cache.reload_all_from_disk();
    c.log("Reloaded");
    Ok(())
});

simple_command!(SaveAllCommand, ["saveall"], "Saves Presto's cache for this world", [
    "The saveall command saves Presto's world cache.",
    "",
    "Usage:",
    "> saveall",
], |c, label, args| {
    args.require_max(0)?;
    let w = c
        .b
        .bot
        .world
        .current()
        .ok_or_else(|| CommandError::InvalidState("You need to be in a world for that".into()))?;
    w.cache.save();
    c.log("Saved");
    Ok(())
});

simple_command!(BlacklistCommand, ["blacklist"], "Blacklist closest block", [
    "While going to a block this command blacklists the closest block so that Presto won't attempt to get to it.",
    "",
    "Usage:",
    "> blacklist",
], |c, label, args| {
    args.require_max(0)?;
    if c.b.procs.get_to_block.getting_to().is_none() {
        return Err(CommandError::InvalidState("GetToBlockProcess is not currently active".into()));
    }
    if c.with_processes(|p, pc| p.get_to_block.blacklist_closest(pc))? {
        c.log("Blacklisted closest instances");
        Ok(())
    } else {
        Err(CommandError::InvalidState("No known locations, unable to blacklist".into()))
    }
});

pub struct FindCommand;

impl Command for FindCommand {
    fn names(&self) -> &[&'static str] {
        &["find"]
    }
    fn execute(&self, c: &mut Ctx, _label: &str, args: &mut Args) -> Result<(), CommandError> {
        args.require_min(1)?;
        let pc = c.pc()?;
        let table = pc.world.table.clone();
        let mut to_find = Vec::new();
        while args.has_any() {
            to_find.push(args.datatype("BlockById", |a| block_by_id(&table, a))?);
        }
        let origin = pc.player_feet();
        let Some(world) = c.b.bot.world.current() else {
            c.log("No positions known, are you sure the blocks are cached?");
            return Ok(());
        };
        let cache = world.cache.clone();
        let mut found = Vec::new();
        for block in to_find {
            let name = table.block(block).name.clone();
            // (Java passes the player's y as the z of the centre; z is what was meant.)
            found.extend(cache.locations_of(&name, usize::MAX, origin.x, origin.z, 4));
        }
        if found.is_empty() {
            c.log("No positions known, are you sure the blocks are cached?");
            return Ok(());
        }
        for pos in found {
            let text = format!("{} {} {}", pos.x, pos.y, pos.z);
            c.log_spans(vec![clickable(
                pos_string(pos),
                chat_color::GRAY,
                format!("{}goal {text}", force_command_prefix()),
                "Click to set goal to this position",
            )]);
        }
        Ok(())
    }
    fn tab_complete(&self, _c: &mut Ctx, _label: &str, args: &mut Args) -> Result<Vec<String>, CommandError> {
        let s = args.get_string()?;
        Ok(Completions::from(crate::cache::chunk::BLOCKS_TO_KEEP_TRACK_OF.iter().map(|b| format!("minecraft:{b}")))
            .filter_prefix_namespaced(&s)
            .sort_alphabetically()
            .build())
    }
    fn short_desc(&self) -> String {
        "Find positions of a certain block".into()
    }
    fn long_desc(&self) -> Vec<String> {
        lines(&[
            "The find command searches through Presto's cache and attempts to find the location of the block.",
            "Tab completion will suggest only cached blocks and uncached blocks can not be found.",
            "",
            "Usage:",
            "> find <block> [...] - Try finding the listed blocks",
        ])
    }
}

pub struct MineCommand;

impl Command for MineCommand {
    fn names(&self) -> &[&'static str] {
        &["mine"]
    }
    fn execute(&self, c: &mut Ctx, _label: &str, args: &mut Args) -> Result<(), CommandError> {
        let quantity = args.get_as_or::<i32>(0)?;
        args.require_min(1)?;
        let pc = c.pc()?;
        let table = pc.world.table.clone();
        let mut boms = Vec::new();
        while args.has_any() {
            boms.push(args.datatype("ForBlockOptionalMeta", |a| block_optional_meta(&table, a))?);
        }
        c.b.repack(pc.game, 40);
        let names: Vec<String> = boms.iter().map(|b| b.to_string()).collect();
        c.log(&format!("Mining [{}]", names.join(", ")));
        c.with_processes(|p, pc| p.mine.mine(pc, quantity, Some(BlockOptionalMetaLookup::new(boms))))?;
        Ok(())
    }
    fn tab_complete(&self, c: &mut Ctx, _label: &str, args: &mut Args) -> Result<Vec<String>, CommandError> {
        let Ok(pc) = c.pc() else { return Ok(Vec::new()) };
        let table = pc.world.table.clone();
        args.get_as_or::<i32>(0)?;
        while args.has(2) {
            args.datatype("ForBlockOptionalMeta", |a| block_optional_meta(&table, a))?;
        }
        Ok(complete_block_optional_meta(&table, args))
    }
    fn short_desc(&self) -> String {
        "Mine some blocks".into()
    }
    fn long_desc(&self) -> Vec<String> {
        lines(&[
            "The mine command allows you to tell Presto to search for and mine individual blocks.",
            "",
            "The specified blocks can be ores, or any other block.",
            "",
            "Also see the legitMine settings (see #set l legitMine).",
            "",
            "Usage:",
            "> mine diamond_ore - Mines all diamonds it can find.",
        ])
    }
}

simple_command!(ClickCommand, ["click"], "Open click", [
    "Opens click dude",
    "",
    "Usage:",
    "> click",
], |c, label, args| {
    args.require_max(0)?;
    c.b.bot.out.open_click = true;
    c.log("aight dude");
    Ok(())
});

simple_command!(SurfaceCommand, ["surface", "top"], "Used to get out of caves, mines, ...", [
    "The surface/top command tells Presto to head towards the closest surface-like area.",
    "",
    "This can be the surface or the highest available air space, depending on circumstances.",
    "",
    "Usage:",
    "> surface - Used to get out of caves, mines, ...",
    "> top - Used to get out of caves, mines, ...",
], |c, label, args| {
    let pc = c.pc()?;
    let feet = pc.player_feet();
    let dim = &pc.world.bsi.dimension;
    let surface = dim.sea_level;
    let world_height = dim.height;
    let air = |p: BlockPos| pc.world.info_pos(p).is_air();
    // Above the surface with air overhead: already on the open surface.
    if feet.y > surface && air(feet.above()) {
        c.log("Already at surface");
        return Ok(());
    }
    for y in feet.y.max(surface)..world_height {
        let p = BlockPos::new(feet.x, y, feet.z);
        if !air(p) && y > feet.y {
            let goal: GoalRef = Arc::new(GoalBlock::at(p.above()));
            c.log(&format!("Going to: {goal}"));
            c.b.procs.custom_goal.set_goal_and_path(goal);
            return Ok(());
        }
    }
    c.log("No higher location found");
    Ok(())
});

simple_command!(ThisWayCommand, ["thisway", "forward"], "Travel in your current direction", [
    "Creates a GoalXZ some amount of blocks in the direction you're currently looking",
    "",
    "Usage:",
    "> thisway <distance> - makes a GoalXZ distance blocks in front of you",
], |c, label, args| {
    args.require_exactly(1)?;
    let pc = c.pc()?;
    let distance = args.get_as::<f64>()?;
    let goal = GoalXZ::from_direction(pc.player.position, pc.player.yaw, distance);
    c.b.procs.custom_goal.set_goal(Some(Arc::new(goal)));
    c.log(&format!("Goal: {goal}"));
    Ok(())
});

simple_command!(PauseCommand, ["pause", "p", "paws"], "Pauses Presto until you use resume", [
    "The pause command tells Presto to temporarily stop whatever it's doing.",
    "",
    "This can be used to pause pathing, building, following, whatever. A single use of the resume command will start it right back up again!",
    "",
    "Usage:",
    "> pause",
], |c, label, args| {
    args.require_max(0)?;
    if c.b.procs.pause.paused {
        return Err(CommandError::InvalidState("Already paused".into()));
    }
    c.b.procs.pause.paused = true;
    c.log("Paused");
    Ok(())
});

simple_command!(ResumeCommand, ["resume", "r", "unpause", "unpaws"], "Resumes Presto after a pause", [
    "The resume command tells Presto to resume whatever it was doing when you last used pause.",
    "",
    "Usage:",
    "> resume",
], |c, label, args| {
    args.require_max(0)?;
    c.b.procs.builder.resume();
    if !c.b.procs.pause.paused {
        return Err(CommandError::InvalidState("Not paused".into()));
    }
    c.b.procs.pause.paused = false;
    c.log("Resumed");
    Ok(())
});

simple_command!(PausedCommand, ["paused"], "Tells you if Presto is paused", [
    "The paused command tells you if Presto is currently paused by use of the pause command.",
    "",
    "Usage:",
    "> paused",
], |c, label, args| {
    args.require_max(0)?;
    let paused = c.b.procs.pause.paused;
    c.log(&format!("Presto is {}paused", if paused { "" } else { "not " }));
    Ok(())
});

simple_command!(CancelCommand, ["cancel", "c", "stop"], "Cancel what Presto is currently doing", [
    "The cancel command tells Presto to stop whatever it's currently doing.",
    "",
    "Usage:",
    "> cancel",
], |c, label, args| {
    args.require_max(0)?;
    c.b.procs.pause.paused = false;
    c.b.cancel_everything();
    c.log("ok canceled");
    Ok(())
});

pub struct ExploreCommand;

impl Command for ExploreCommand {
    fn names(&self) -> &[&'static str] {
        &["explore"]
    }
    fn execute(&self, c: &mut Ctx, _label: &str, args: &mut Args) -> Result<(), CommandError> {
        if args.has_any() {
            args.require_exactly(2)?;
        } else {
            args.require_max(0)?;
        }
        let feet = c.pc()?.player_feet();
        let goal = if args.has_any() {
            args.datatype("RelativeGoalXZ", |a| relative_goal_xz(a, feet))?
        } else {
            GoalXZ::new(feet.x, feet.z)
        };
        c.b.procs.explore.explore(goal.x, goal.z);
        c.log(&format!("Exploring from {goal}"));
        Ok(())
    }
    fn tab_complete(&self, _c: &mut Ctx, _label: &str, args: &mut Args) -> Result<Vec<String>, CommandError> {
        if args.has_at_most(2) {
            while args.has(2) {
                args.get()?;
            }
            return Ok(complete_relative_coordinate(args));
        }
        Ok(Vec::new())
    }
    fn short_desc(&self) -> String {
        "Explore things".into()
    }
    fn long_desc(&self) -> Vec<String> {
        lines(&[
            "Tell Presto to explore randomly. If you used explorefilter before this, it will be applied.",
            "",
            "Usage:",
            "> explore - Explore from your current position.",
            "> explore <x> <z> - Explore from the specified X and Z position.",
        ])
    }
}

pub struct ExploreFilterCommand;

impl Command for ExploreFilterCommand {
    fn names(&self) -> &[&'static str] {
        &["explorefilter"]
    }
    fn execute(&self, c: &mut Ctx, _label: &str, args: &mut Args) -> Result<(), CommandError> {
        args.require_max(2)?;
        let name = args.get_string()?;
        // Relative to the folder the game's folder is in, as Baritone reads it.
        let base = c
            .b
            .bot
            .directory
            .parent()
            .and_then(|game| game.parent())
            .map(std::path::Path::to_path_buf)
            .unwrap_or_default();
        let path = base.join(&name);
        let mut invert = false;
        if args.has_any() {
            if args.get_string()?.eq_ignore_ascii_case("invert") {
                invert = true;
            } else {
                return Err(CommandError::invalid_type(&args.consumed(), "either \"invert\" or nothing"));
            }
        }
        let text = std::fs::read_to_string(&path).map_err(|_| CommandError::InvalidState("File not found".into()))?;
        let filter = crate::process::explore::ChunkFilter::parse(&text, invert).map_err(CommandError::InvalidState)?;
        c.log(&format!("Loaded {} positions", filter.len()));
        c.b.procs.explore.apply_filter(filter);
        c.log(&format!("Explore filter applied. Inverted: {invert}"));
        Ok(())
    }
    fn short_desc(&self) -> String {
        "Explore chunks from a json".into()
    }
    fn long_desc(&self) -> Vec<String> {
        lines(&[
            "Apply an explore filter before using explore, which tells the explore process which chunks have been explored/not explored.",
            "",
            "The JSON file will follow this format: [{\"x\":0,\"z\":0},...]",
            "",
            "If 'invert' is specified, the chunks listed will be considered NOT explored, rather than explored.",
            "",
            "Usage:",
            "> explorefilter <path> [invert] - Load the JSON file referenced by the specified path. If invert is specified, it must be the literal word 'invert'.",
        ])
    }
}
