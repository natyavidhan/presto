//! `CustomGoalProcess`: go to a goal the player gave (`#goto`, `#goal` + `#path`, `#come`...).

use super::{PathingCommand, PathingCommandType, Process, ProcessCtx};
use crate::behavior::pathing::PathingBehavior;
use crate::bot::Bot;
use crate::pathing::goals::GoalRef;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum State {
    #[default]
    None,
    GoalSet,
    PathRequested,
    Executing,
}

#[derive(Default)]
pub struct CustomGoalProcess {
    goal: Option<GoalRef>,
    most_recent_goal: Option<GoalRef>,
    state: State,
    /// Set when a goal arrives while elytra flight is active, for it to retarget (`ElytraProcess.pathTo`).
    pub elytra_retarget: Option<GoalRef>,
}

impl CustomGoalProcess {
    /// `setGoal`.
    pub fn set_goal(&mut self, goal: Option<GoalRef>) {
        self.goal = goal.clone();
        self.most_recent_goal = goal.clone();
        if let Some(g) = goal {
            self.elytra_retarget = Some(g);
        }
        if self.state == State::None {
            self.state = State::GoalSet;
        }
        if self.state == State::Executing {
            self.state = State::PathRequested;
        }
    }

    /// `path`: start pathing to the goal.
    pub fn path(&mut self) {
        self.state = State::PathRequested;
    }

    /// `setGoalAndPath`.
    pub fn set_goal_and_path(&mut self, goal: GoalRef) {
        self.set_goal(Some(goal));
        self.path();
    }

    pub fn goal(&self) -> Option<&GoalRef> {
        self.goal.as_ref()
    }

    pub fn most_recent_goal(&self) -> Option<&GoalRef> {
        self.most_recent_goal.as_ref()
    }

    pub fn is_active_now(&self) -> bool {
        self.state != State::None
    }
}

impl Process for CustomGoalProcess {
    fn is_active(&mut self, _c: &mut ProcessCtx) -> bool {
        self.state != State::None
    }

    fn on_tick(&mut self, c: &mut ProcessCtx, calc_failed: bool, _is_safe_to_cancel: bool) -> Option<PathingCommand> {
        match self.state {
            State::GoalSet => Some(PathingCommand::new(self.goal.clone(), PathingCommandType::CancelAndSetGoal)),
            State::PathRequested => {
                // FORCE_REVALIDATE_GOAL_AND_PATH just once.
                self.state = State::Executing;
                Some(PathingCommand::new(
                    self.goal.clone(),
                    PathingCommandType::ForceRevalidateGoalAndPath,
                ))
            }
            State::Executing => {
                if calc_failed {
                    let goal = self.goal.clone();
                    self.on_lost_control(c.bot);
                    return Some(PathingCommand::new(goal, PathingCommandType::CancelAndSetGoal));
                }
                let feet = c.pc.player_feet();
                let start = PathingBehavior::path_start(c.pc);
                let arrived = self
                    .goal
                    .as_ref()
                    .is_none_or(|g| g.is_in_goal_pos(feet) && g.is_in_goal_pos(start));
                if arrived {
                    let goal = self.goal.clone();
                    self.on_lost_control(c.bot); // we're there xd
                    if c.bot.settings.disconnect_on_arrival {
                        c.bot.out.disconnect = Some("[Presto] Arrived at goal!".into());
                    }
                    if c.bot.settings.notification_on_path_complete {
                        c.bot.log_notification("Pathing complete", false);
                    }
                    return Some(PathingCommand::new(goal, PathingCommandType::CancelAndSetGoal));
                }
                Some(PathingCommand::new(self.goal.clone(), PathingCommandType::SetGoalAndPath))
            }
            State::None => None,
        }
    }

    fn on_lost_control(&mut self, _bot: &mut Bot) {
        self.state = State::None;
        self.goal = None;
    }

    fn display_name(&self) -> String {
        match &self.goal {
            Some(g) => format!("Custom Goal {g}"),
            None => "Custom Goal null".into(),
        }
    }
}
