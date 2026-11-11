//! Processes (`baritone.process`) and `PathingControlManager`: the things Baritone can be doing (going to a
//! goal, mining, following, building...), one in control at a time by priority, each tick telling the
//! pathing behavior what goal to path to.

pub mod backfill;
pub mod builder;
pub mod custom_goal;
pub mod elytra;
pub mod explore;
pub mod farm;
pub mod follow;
pub mod get_to_block;
pub mod inventory_pauser;
pub mod mine;

use crate::behavior::pathing::PathingBehavior;
use crate::bot::Bot;
use crate::ctx::PlayerContext;
use crate::pathing::context::Protection;
use crate::pathing::movement::Exec;
use crate::pathing::goals::{GoalRef, goals_equal};
use std::sync::Arc;

/// `IBaritoneProcess.DEFAULT_PRIORITY`.
pub const DEFAULT_PRIORITY: f64 = -1.0;

/// `PathingCommandType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathingCommandType {
    /// Set the goal and path to it.
    SetGoalAndPath,
    /// Fully stop, but only once it's safe.
    RequestPause,
    /// Set the goal and cancel the current path (once it's safe).
    CancelAndSetGoal,
    /// Path to the goal; cancel the current path if it no longer leads there (`cancelOnGoalInvalidation`).
    RevalidateGoalAndPath,
    /// Path to the goal; cancel the current path unless it leads there.
    ForceRevalidateGoalAndPath,
    /// Let the next process decide.
    Defer,
    /// Set the goal, path to it, and pause.
    SetGoalAndPause,
}

/// `PathingCommand` (and `PathingCommandContext`'s cost overrides).
#[derive(Clone)]
pub struct PathingCommand {
    pub goal: Option<GoalRef>,
    pub kind: PathingCommandType,
    pub protection: Option<Arc<dyn Protection>>,
}

impl PathingCommand {
    pub fn new(goal: Option<GoalRef>, kind: PathingCommandType) -> Self {
        Self {
            goal,
            kind,
            protection: None,
        }
    }
    pub fn with_protection(goal: Option<GoalRef>, kind: PathingCommandType, protection: Arc<dyn Protection>) -> Self {
        Self {
            goal,
            kind,
            protection: Some(protection),
        }
    }
}

impl std::fmt::Debug for PathingCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.goal {
            Some(g) => write!(f, "{:?} {g}", self.kind),
            None => write!(f, "{:?} null", self.kind),
        }
    }
}

/// What a process works with during its tick.
pub struct ProcessCtx<'a, 'b> {
    pub bot: &'a mut Bot,
    pub pc: &'a PlayerContext<'b>,
}

impl ProcessCtx<'_, '_> {
    /// Runs movement code (`MovementHelper`: placing a block, picking a tool...) from a process.
    pub fn with_exec<R>(&mut self, f: impl FnOnce(&mut Exec) -> R) -> R {
        let mut debug = Vec::new();
        let no_schematic = |_: i32, _: i32, _: i32| None;
        let bot = &mut *self.bot;
        let mut ex = Exec {
            ctx: self.pc,
            look: &mut bot.look,
            input: &mut bot.input,
            inventory: &mut bot.inventory,
            path_start: PathingBehavior::path_start(self.pc),
            builder_place_at: &no_schematic,
            container_open: self.pc.game.container_open,
            stationary: bot.pauser.stationary(),
            debug: &mut debug,
            stop_flying: false,
        };
        let r = f(&mut ex);
        bot.out.chat.extend(debug);
        r
    }
}

/// `IBaritoneProcess`.
pub trait Process {
    fn is_active(&mut self, c: &mut ProcessCtx) -> bool;
    /// `onTick`: what to do this tick; `None` only when the process has just become inactive.
    fn on_tick(&mut self, c: &mut ProcessCtx, calc_failed: bool, is_safe_to_cancel: bool) -> Option<PathingCommand>;
    /// A temporary process (a pause) doesn't take control away from the others for good.
    fn is_temporary(&self) -> bool {
        false
    }
    fn on_lost_control(&mut self, bot: &mut Bot);
    fn priority(&self) -> f64 {
        DEFAULT_PRIORITY
    }
    fn display_name(&self) -> String;
}

/// Which process.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProcessId {
    Follow,
    Mine,
    CustomGoal,
    GetToBlock,
    Builder,
    Explore,
    Farm,
    InventoryPauser,
    Elytra,
    Backfill,
}

impl ProcessId {
    /// Registration order (`Baritone`'s constructor).
    pub const ALL: [ProcessId; 10] = [
        ProcessId::Follow,
        ProcessId::Mine,
        ProcessId::CustomGoal,
        ProcessId::GetToBlock,
        ProcessId::Builder,
        ProcessId::Explore,
        ProcessId::Farm,
        ProcessId::InventoryPauser,
        ProcessId::Elytra,
        ProcessId::Backfill,
    ];
}

/// Every process.
#[derive(Default)]
pub struct Processes {
    pub follow: follow::FollowProcess,
    pub mine: mine::MineProcess,
    pub custom_goal: custom_goal::CustomGoalProcess,
    pub get_to_block: get_to_block::GetToBlockProcess,
    pub builder: builder::BuilderProcess,
    pub explore: explore::ExploreProcess,
    pub farm: farm::FarmProcess,
    pub inventory_pauser: inventory_pauser::InventoryPauserProcess,
    pub elytra: elytra::ElytraProcess,
    pub backfill: backfill::BackfillProcess,
}

impl Processes {
    pub fn get_mut(&mut self, id: ProcessId) -> &mut dyn Process {
        match id {
            ProcessId::Follow => &mut self.follow,
            ProcessId::Mine => &mut self.mine,
            ProcessId::CustomGoal => &mut self.custom_goal,
            ProcessId::GetToBlock => &mut self.get_to_block,
            ProcessId::Builder => &mut self.builder,
            ProcessId::Explore => &mut self.explore,
            ProcessId::Farm => &mut self.farm,
            ProcessId::InventoryPauser => &mut self.inventory_pauser,
            ProcessId::Elytra => &mut self.elytra,
            ProcessId::Backfill => &mut self.backfill,
        }
    }
}

/// `PathingControlManager`.
#[derive(Default)]
pub struct PathingControlManager {
    active: Vec<ProcessId>,
    in_control_last_tick: Option<ProcessId>,
    in_control_this_tick: Option<ProcessId>,
    command: Option<PathingCommand>,
}

impl PathingControlManager {
    pub fn most_recent_in_control(&self) -> Option<ProcessId> {
        self.in_control_this_tick
    }

    pub fn most_recent_command(&self) -> Option<&PathingCommand> {
        self.command.as_ref()
    }

    /// `cancelEverything`.
    pub fn cancel_everything(&mut self, procs: &mut Processes, bot: &mut Bot) {
        self.in_control_last_tick = None;
        self.in_control_this_tick = None;
        self.command = None;
        self.active.clear();
        for id in ProcessId::ALL {
            procs.get_mut(id).on_lost_control(bot);
        }
    }

    /// `preTick`: run the processes and act on the command of the one in control.
    pub fn pre_tick(&mut self, procs: &mut Processes, bot: &mut Bot, pc: &PlayerContext, has_throwaway: bool) {
        self.in_control_last_tick = self.in_control_this_tick;
        self.in_control_this_tick = None;
        self.command = self.execute_processes(procs, bot, pc);
        let Some(command) = self.command.clone() else {
            bot.pathing.cancel_segment_if_safe(&mut bot.input);
            bot.pathing.secret_internal_set_goal(None);
            return;
        };
        if self.in_control_this_tick != self.in_control_last_tick
            && command.kind != PathingCommandType::RequestPause
            && let Some(last) = self.in_control_last_tick
            && !procs.get_mut(last).is_temporary()
        {
            // Control moved from one real process to another that wants something done: drop what the last
            // one had in progress.
            bot.pathing.cancel_segment_if_safe(&mut bot.input);
        }
        match command.kind {
            PathingCommandType::SetGoalAndPause => {
                bot.pathing
                    .secret_internal_set_goal_and_path(command.goal.clone(), command.protection.clone(), pc, has_throwaway);
                bot.pathing.request_pause();
            }
            PathingCommandType::RequestPause => bot.pathing.request_pause(),
            PathingCommandType::CancelAndSetGoal => {
                bot.pathing.secret_internal_set_goal(command.goal.clone());
                bot.pathing.cancel_segment_if_safe(&mut bot.input);
            }
            PathingCommandType::ForceRevalidateGoalAndPath | PathingCommandType::RevalidateGoalAndPath => {
                if !bot.pathing.is_pathing() && bot.pathing.in_progress().is_none() {
                    bot.pathing
                        .secret_internal_set_goal_and_path(command.goal.clone(), command.protection.clone(), pc, has_throwaway);
                }
            }
            PathingCommandType::SetGoalAndPath => {
                if command.goal.is_some() {
                    bot.pathing
                        .secret_internal_set_goal_and_path(command.goal.clone(), command.protection.clone(), pc, has_throwaway);
                }
            }
            PathingCommandType::Defer => unreachable!("a deferring process is never in control"),
        }
    }

    /// `postTick`: cancel and recalculate after the tick's path has run, so the time between ticks is
    /// calculation time.
    pub fn post_tick(&mut self, bot: &mut Bot, pc: &PlayerContext, has_throwaway: bool) {
        let Some(command) = self.command.clone() else {
            return;
        };
        match command.kind {
            PathingCommandType::ForceRevalidateGoalAndPath => {
                if command.goal.as_ref().is_none_or(|g| force_revalidate(bot, g) || revalidate_goal(bot, g)) {
                    bot.pathing.soft_cancel_if_safe();
                }
                bot.pathing
                    .secret_internal_set_goal_and_path(command.goal.clone(), command.protection.clone(), pc, has_throwaway);
            }
            PathingCommandType::RevalidateGoalAndPath => {
                if bot.settings.cancel_on_goal_invalidation
                    && command.goal.as_ref().is_none_or(|g| revalidate_goal(bot, g))
                {
                    bot.pathing.soft_cancel_if_safe();
                }
                bot.pathing
                    .secret_internal_set_goal_and_path(command.goal.clone(), command.protection.clone(), pc, has_throwaway);
            }
            _ => {}
        }
    }

    /// `executeProcesses`: the highest priority active process that doesn't defer.
    fn execute_processes(&mut self, procs: &mut Processes, bot: &mut Bot, pc: &PlayerContext) -> Option<PathingCommand> {
        for id in ProcessId::ALL {
            let active = {
                let mut c = ProcessCtx { bot: &mut *bot, pc };
                procs.get_mut(id).is_active(&mut c)
            };
            if active {
                if !self.active.contains(&id) {
                    // A newly active process goes to the very front.
                    self.active.insert(0, id);
                }
            } else {
                self.active.retain(|p| *p != id);
            }
        }
        // Highest priority first; a stable sort keeps the earlier added first among equals.
        let mut keyed: Vec<(f64, ProcessId)> = self.active.iter().map(|id| (procs.get_mut(*id).priority(), *id)).collect();
        keyed.sort_by(|a, b| b.0.total_cmp(&a.0));
        self.active = keyed.into_iter().map(|(_, id)| id).collect();

        let calc_failed = bot.pathing.calc_failed_last_tick();
        let is_safe_to_cancel = bot.pathing.is_safe_to_cancel() && (!procs.elytra.is_active_now() || procs.elytra.is_safe_to_cancel());
        let order = self.active.clone();
        for (i, id) in order.iter().enumerate() {
            let exec = {
                let mut c = ProcessCtx { bot: &mut *bot, pc };
                let proc = procs.get_mut(*id);
                proc.on_tick(&mut c, Some(*id) == self.in_control_last_tick && calc_failed, is_safe_to_cancel)
            };
            match exec {
                None => {
                    // Reporting inactive; no need to call on_lost_control.
                }
                Some(cmd) if cmd.kind != PathingCommandType::Defer => {
                    self.in_control_this_tick = Some(*id);
                    if !procs.get_mut(*id).is_temporary() {
                        for rest in &order[i + 1..] {
                            procs.get_mut(*rest).on_lost_control(bot);
                        }
                    }
                    return Some(cmd);
                }
                Some(_) => {}
            }
        }
        None
    }
}

/// `forceRevalidate`: the current path's goal isn't this one, and it doesn't end in this one.
fn force_revalidate(bot: &Bot, new_goal: &GoalRef) -> bool {
    match bot.pathing.current() {
        Some(cur) => {
            if new_goal.is_in_goal_pos(cur.path().dest()) {
                return false;
            }
            !goals_equal(Some(new_goal), Some(&cur.path().goal))
        }
        None => false,
    }
}

/// `revalidateGoal`: the current path used to end in its goal, but doesn't end in the new one.
fn revalidate_goal(bot: &Bot, new_goal: &GoalRef) -> bool {
    match bot.pathing.current() {
        Some(cur) => {
            let end = cur.path().dest();
            cur.path().goal.is_in_goal_pos(end) && !new_goal.is_in_goal_pos(end)
        }
        None => false,
    }
}
