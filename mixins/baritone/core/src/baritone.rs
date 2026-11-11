//! `Baritone` and `GameEventHandler`: the bot as a whole, and the order its parts run in each tick.
//!
//! The game calls [`Baritone::tick`] once per client tick, before the player moves and before the tick's clicks
//! are handled, with what the game looks like; Baritone answers with [`Controls`]. Everything Java Baritone does
//! in `TickEvent` and `PlayerUpdateEvent` happens in that one call, in Java's order:
//!
//! 1. `LookBehavior.onTick`: new random aim offsets.
//! 2. `PathingBehavior.onTick`: finished searches are picked up, the processes run (`preTick`), the path is
//!    walked (`tickPath`).
//! 3. `InventoryBehavior.onTick`.
//! 4. `InputOverrideHandler.onTick`: the break and place helpers and the movement keys, with the crosshair
//!    where this tick's turn puts it, so a block can be hit or placed against in the tick Baritone turns to it.
//! 5. `PathingControlManager.postTick`: cancels and new searches, so they run while the game does the rest.
//! 6. `LookBehavior.onPlayerUpdate`: the turn the game applies before the player moves.

use crate::behavior::inventory::{InvCtx, has_generic_throwaway};
use crate::behavior::pathing::{PathEvent, PathingBehavior};
use crate::bot::Bot;
use crate::cache::CacheConfig;
use crate::cache::chunk::keeps_track_of;
use crate::cache::waypoints::{Tag, Waypoint};
use crate::ctx::PlayerContext;
use crate::math::BlockPos;
use crate::pathing::goals;
use crate::pathing::movement::Exec;
use crate::player::{Controls, GameState};
use crate::process::{PathingControlManager, ProcessCtx, Processes};
use crate::settings::{self, Settings};
use crate::utils::output::{Output, force_command_prefix};
use crate::world::{BlockSource, StateId, flags};
use std::path::PathBuf;
use std::sync::Arc;

/// A command waiting for the next tick.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Pending {
    /// Typed with the prefix (`#goto 1 2 3`): an unknown one is reported.
    Prefixed(String),
    /// Typed without it, with `chatControl` (`goto 1 2 3`).
    Chat(String),
    /// From Baritone's own clickable chat, or `/baritone`: run as it is.
    Direct(String),
}

pub struct Baritone {
    pub bot: Bot,
    pub procs: Processes,
    pub control: PathingControlManager,
    /// Commands typed in the chat, run at the start of the next tick.
    pending_commands: Vec<Pending>,
    /// Waypoints deleted with `#wp delete` and `#wp clear`, by world, for `#wp restore`.
    deleted_waypoints: std::collections::HashMap<PathBuf, Vec<Waypoint>>,
    /// The settings the globals (goal heuristics, coordinate censoring, the cache's config) were set from.
    applied_settings: Option<Arc<Settings>>,
    /// The path events of the last tick (`PathEvent`), for the renderer and the GUI.
    pub events: Vec<PathEvent>,
}

impl Baritone {
    /// `directory` is `<game dir>/baritone`; `settings.txt` there is read now (`SettingsUtil.readAndApply`).
    pub fn new(directory: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&directory);
        let mut settings = Settings::default();
        let mut errors = Vec::new();
        if let Ok(text) = std::fs::read_to_string(directory.join("settings.txt")) {
            errors = settings.apply_file(&text);
        }
        let mut bot = Bot::new(directory, settings);
        for e in errors {
            log::warn!("[Baritone] settings.txt: {e}");
        }
        bot.out = Output::default();
        Self {
            bot,
            procs: Processes::default(),
            control: PathingControlManager::default(),
            pending_commands: Vec::new(),
            deleted_waypoints: Default::default(),
            applied_settings: None,
            events: Vec::new(),
        }
    }

    pub fn settings(&self) -> &Arc<Settings> {
        &self.bot.settings
    }

    /// Settings read by code with no `Settings` at hand are copied to globals when they change.
    fn apply_settings_globals(&mut self) {
        let s = &self.bot.settings;
        if self.applied_settings.as_ref().is_some_and(|a| Arc::ptr_eq(a, s)) {
            return;
        }
        goals::set_goal_settings(s.cost_heuristic, s.axis_height);
        settings::set_censor_coordinates(s.censor_coordinates);
        if let Some(w) = self.bot.world.current() {
            w.cache.set_config(CacheConfig::from_settings(s));
        }
        self.applied_settings = Some(s.clone());
    }

    /// `onWorldEvent`: opens the world data of the world and dimension the player is in, and hands the
    /// cache to this tick's world view so searches can read chunks seen before.
    fn update_world(&mut self, game: &mut GameState) {
        let (Some(view), Some(dir)) = (game.world.as_mut(), game.world_dir.as_ref()) else {
            if game.world.is_none() {
                self.bot.world.close_world();
            }
            return;
        };
        let config = CacheConfig::from_settings(&self.bot.settings);
        if self.bot.world.ensure(dir, &view.dimension, &view.table, config) {
            self.bot.look.on_world_change();
        }
        if let Some(w) = self.bot.world.current() {
            view.cache = Some(w.cache.clone());
            if let Some(p) = &game.player {
                w.cache.set_player_position(p.feet_raw());
            }
        }
    }

    /// One client tick.
    pub fn tick(&mut self, game: &mut GameState) -> Controls {
        self.apply_settings_globals();
        self.update_world(game);
        let game: &GameState = game;
        let settings = self.bot.settings.clone();
        let has_throwaway = has_generic_throwaway(&game.inventory, &settings);
        let Some(pc) = PlayerContext::new(game, &settings, has_throwaway) else {
            // TickEvent OUT.
            self.run_commands(None);
            self.events = self.bot.pathing.dispatch_events();
            self.bot.pathing.secret_internal_segment_cancel(&mut self.bot.input);
            self.control.cancel_everything(&mut self.procs, &mut self.bot);
            return self.finish(Controls::default());
        };
        self.run_commands(Some(&pc));
        self.bot.inventory.begin_tick();

        // LookBehavior.onTick.
        self.bot.look.tick(&settings);

        // PathingBehavior.onTick. Events are dispatched once a tick, here, so a failed search is seen by the
        // next preTick wherever in the last tick it ended.
        self.bot.pathing.poll_search();
        self.events = self.bot.pathing.dispatch_events();
        for line in std::mem::take(&mut self.bot.pathing.debug) {
            self.bot.log_debug(&line);
        }
        let path_start = PathingBehavior::path_start(&pc);
        self.bot.pathing.set_expected_segment_start(path_start);
        self.control.pre_tick(&mut self.procs, &mut self.bot, &pc, has_throwaway);
        let stop_flying = self.tick_path(&pc, has_throwaway);
        self.bot.pathing.tick_elapsed();
        for line in std::mem::take(&mut self.bot.pathing.debug) {
            self.bot.log_debug(&line);
        }

        // InventoryBehavior.onTick.
        let inv = InvCtx {
            inv: &game.inventory,
            settings: &settings,
            container_open: game.container_open,
            stationary: self.bot.pauser.stationary(),
        };
        self.bot.inventory.on_tick(&inv, &pc.world.table);

        // The turn (LookBehavior.onPlayerUpdate) is settled now: nothing after this sets a target.
        let look = self.bot.look.take_command(
            pc.player_rotations(),
            &settings,
            game.mouse_sensitivity,
            pc.player.fall_flying,
        );
        // Where the crosshair will be once the game has made that turn.
        let aimed = match &look {
            Some(cmd) => pc
                .ray_trace_towards(cmd.rotation, game.block_interaction_range, pc.player.crouching)
                .block_pos(),
            None => pc.selected_block(),
        };

        // InputOverrideHandler.onTick.
        let block_at = |p: BlockPos| pc.world.info_pos(p).block;
        let decision = self.bot.input.tick(
            aimed,
            &block_at,
            pc.player.hands_busy,
            self.bot.pathing.is_pathing(),
            self.bot.pathing.is_sprinting(),
            &settings,
        );

        // PathingControlManager.postTick.
        self.control.post_tick(&mut self.bot, &pc, has_throwaway);

        // An inventory move that waited for the player to stand still asks the inventory pauser for a stop.
        if self.bot.inventory.pause_requested {
            self.bot.pauser.pause_requested_last_tick = true;
        }

        let controls = Controls {
            look,
            movement: decision.movement,
            attack: decision.attack,
            use_item: decision.use_item,
            select_slot: self.bot.inventory.selected,
            inventory_clicks: std::mem::take(&mut self.bot.inventory.clicks),
            stop_flying,
            ..Controls::default()
        };
        self.finish(controls)
    }

    /// `tickPath`, with what the movements may touch. Returns whether a movement wants creative flight off.
    fn tick_path(&mut self, pc: &PlayerContext, has_throwaway: bool) -> bool {
        let bot = &mut self.bot;
        let builder = &self.procs.builder;
        let place_at = |x: i32, y: i32, z: i32| builder.place_at(x, y, z);
        let mut debug = Vec::new();
        let mut ex = Exec {
            ctx: pc,
            look: &mut bot.look,
            input: &mut bot.input,
            inventory: &mut bot.inventory,
            path_start: bot.pathing.expected_segment_start(),
            builder_place_at: &place_at,
            container_open: pc.game.container_open,
            stationary: bot.pauser.stationary(),
            debug: &mut debug,
            stop_flying: false,
        };
        bot.pathing.tick_path(&mut ex, has_throwaway);
        let stop_flying = ex.stop_flying;
        bot.out.chat.extend(debug);
        stop_flying
    }

    /// Moves what Baritone said this tick into the controls.
    fn finish(&mut self, mut controls: Controls) -> Controls {
        let out = self.bot.out.take();
        controls.chat = out.chat;
        controls.notifications = out.notifications;
        controls.toasts = out.toasts;
        controls.disconnect = out.disconnect;
        controls.rerender_chunks = out.rerender_chunks;
        controls.open_click = out.open_click;
        controls
    }

    /// A [`ProcessCtx`] for calls into the processes from outside the tick (commands).
    pub fn process_ctx<'a, 'b>(&'a mut self, pc: &'a PlayerContext<'b>) -> ProcessCtx<'a, 'b> {
        ProcessCtx { bot: &mut self.bot, pc }
    }

    fn run_commands(&mut self, pc: Option<&PlayerContext>) {
        for p in std::mem::take(&mut self.pending_commands) {
            match p {
                Pending::Prefixed(cmd) => {
                    if !crate::command::run_command(self, pc, &cmd) && !cmd.trim().is_empty() {
                        let label = crate::command::args::expand(&cmd, false).0;
                        crate::command::CommandError::NotFound(label).handle(self);
                    }
                }
                Pending::Chat(msg) => {
                    crate::command::run_command(self, pc, &msg);
                }
                Pending::Direct(cmd) => {
                    crate::command::execute(self, pc, &cmd);
                }
            }
        }
    }

    /// `onSendChatMessage`: chat the player is about to send. Baritone takes `#commands` (with `prefixControl`),
    /// the commands its own clickable chat runs, and (with `chatControl`) any line that is a command or a
    /// setting; it returns true for those, which then aren't sent. They run at the start of the next tick.
    pub fn on_send_chat(&mut self, message: &str) -> bool {
        let s = self.bot.settings.clone();
        let force = message.starts_with(force_command_prefix());
        if (s.prefix_control && message.starts_with(s.prefix.as_str())) || force {
            let cmd = if force {
                &message[force_command_prefix().len()..]
            } else {
                &message[s.prefix.len()..]
            };
            self.pending_commands.push(Pending::Prefixed(cmd.to_string()));
            return true;
        }
        if s.chat_control || s.chat_control_anyway {
            if message.trim().eq_ignore_ascii_case("damn") {
                self.bot.log_direct("daniel");
                return false;
            }
            if crate::command::is_command(message) {
                self.pending_commands.push(Pending::Chat(message.to_string()));
                return true;
            }
        }
        false
    }

    /// A click on a chat line that runs `command` (`MixinScreen.handleCustomClickEvent`): true when it is one
    /// of Baritone's.
    pub fn on_chat_click(&mut self, command: &str) -> bool {
        match command.strip_prefix(force_command_prefix()) {
            Some(cmd) => {
                self.pending_commands.push(Pending::Direct(cmd.to_string()));
                true
            }
            None => false,
        }
    }

    /// `/baritone <args>`: a command typed as a game command.
    pub fn on_command(&mut self, args: &str) {
        self.pending_commands.push(Pending::Prefixed(args.to_string()));
    }

    /// `onPreTabComplete`: completions for what is typed in the chat box, when it starts with the prefix.
    pub fn tab_complete(&mut self, game: &GameState, typed: &str) -> Vec<String> {
        let s = self.bot.settings.clone();
        if !s.prefix_control {
            return Vec::new();
        }
        let Some(msg) = typed.strip_prefix(s.prefix.as_str()) else {
            return Vec::new();
        };
        let has_throwaway = has_generic_throwaway(&game.inventory, &s);
        let pc = PlayerContext::new(game, &s, has_throwaway);
        let words = crate::command::args::split(msg, true).len();
        let mut c = crate::command::Ctx { b: self, pc: pc.as_ref() };
        let completions = crate::command::tab_complete(&mut c, msg);
        if words == 1 {
            completions.into_iter().map(|x| format!("{}{x}", s.prefix)).collect()
        } else {
            completions
        }
    }

    /// `PathingBehavior.cancelEverything`: the path, if it's safe to stop it, and every process.
    pub fn cancel_everything(&mut self) -> bool {
        let do_it = self.bot.pathing.is_safe_to_cancel() && self.procs.elytra.is_safe_to_cancel();
        if do_it {
            self.bot.pathing.secret_internal_segment_cancel(&mut self.bot.input);
        }
        self.control.cancel_everything(&mut self.procs, &mut self.bot);
        do_it
    }

    /// `WorldScanner.repack`: queues every loaded chunk within `range` chunks of the player for the cache.
    pub fn repack(&self, game: &GameState, range: i32) -> usize {
        let (Some(view), Some(player)) = (&game.world, &game.player) else {
            return 0;
        };
        let Some(w) = self.bot.world.current() else { return 0 };
        let feet = player.feet_raw();
        let (pcx, pcz) = (feet.x >> 4, feet.z >> 4);
        let mut queued = 0;
        for cx in pcx - range..=pcx + range {
            for cz in pcz - range..=pcz + range {
                if view.source.is_loaded(cx << 4, cz << 4) {
                    w.cache.queue_for_packing(cx, cz, view.source.clone());
                    queued += 1;
                }
            }
        }
        queued
    }

    /// The waypoints deleted in this world, newest last.
    pub fn deleted_waypoints(&mut self) -> &mut Vec<Waypoint> {
        let key = self
            .bot
            .world
            .current()
            .map(|w| w.directory.clone())
            .unwrap_or_default();
        self.deleted_waypoints.entry(key).or_default()
    }

    /// `onChunkEvent`: a chunk finished loading, or is about to unload. The cache packs it in the background.
    pub fn on_chunk(&self, cx: i32, cz: i32, source: Arc<dyn BlockSource>) {
        if let Some(w) = self.bot.world.current() {
            w.cache.queue_for_packing(cx, cz, source);
        }
    }

    /// `onBlockChange`: with `repackOnAnyBlockChange`, a chunk where a block the cache keeps track of (a chest,
    /// a portal...) appeared is packed again.
    pub fn on_block_change(&self, cx: i32, cz: i32, new_states: &[StateId], source: Arc<dyn BlockSource>) {
        if !self.bot.settings.repack_on_any_block_change {
            return;
        }
        let Some(w) = self.bot.world.current() else { return };
        let table = w.cache.table();
        if new_states.iter().any(|s| keeps_track_of(&table.get(*s).name)) {
            w.cache.queue_for_packing(cx, cz, source);
        }
    }

    /// `WaypointBehavior.onPlayerDeath`: a `death` waypoint where the player died.
    pub fn on_player_death(&mut self, game: &GameState) {
        if !self.bot.settings.do_death_waypoints {
            return;
        }
        let settings = self.bot.settings.clone();
        let Some(pc) = PlayerContext::new(game, &settings, false) else { return };
        let Some(world) = self.bot.world.current_mut() else { return };
        let waypoint = Waypoint::new("death", Tag::Death, pc.player_feet());
        let command = format!(
            "{}wp goto {} @ {}",
            force_command_prefix(),
            waypoint.tag.name(),
            waypoint.creation_timestamp
        );
        world.waypoints.add(waypoint);
        let span = Output::clickable(
            "Death position saved.",
            crate::player::chat_color::WHITE,
            command,
            "Click to goto death",
        );
        self.bot.out.log_spans(&settings, vec![span]);
    }

    /// `WaypointBehavior.onBlockInteract`: using a bed remembers it as a `bed` waypoint (its head).
    pub fn on_block_use(&mut self, pos: BlockPos, game: &GameState) {
        if !self.bot.settings.do_bed_waypoints {
            return;
        }
        let Some(view) = &game.world else { return };
        let info = view.table.get(view.source.state(pos.x, pos.y, pos.z));
        if !info.is(flags::BED) {
            return;
        }
        let mut pos = pos;
        if info.property("part") == Some("foot")
            && let Some(facing) = info.facing
        {
            pos = pos.relative(facing);
        }
        let Some(world) = self.bot.world.current_mut() else { return };
        if !world.waypoints.by_tag(Tag::Bed).any(|w| w.location == pos) {
            world.waypoints.add(Waypoint::new("bed", Tag::Bed, pos));
        }
    }

    /// Saves the cache and closes the world (the game is quitting).
    pub fn shutdown(&mut self) {
        self.bot.world.close_world();
    }
}
