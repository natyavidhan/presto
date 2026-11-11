//! `InputOverrideHandler`, `BlockBreakHelper` and `BlockPlaceHelper`: the keys Baritone holds for the player.

use crate::player::MovementKeys;
use crate::settings::Settings;

/// `baritone.api.utils.input.Input`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Input {
    MoveForward,
    MoveBack,
    MoveLeft,
    MoveRight,
    ClickLeft,
    ClickRight,
    Jump,
    Sneak,
    Sprint,
}

impl Input {
    pub const ALL: [Input; 9] = [
        Input::MoveForward,
        Input::MoveBack,
        Input::MoveLeft,
        Input::MoveRight,
        Input::ClickLeft,
        Input::ClickRight,
        Input::Jump,
        Input::Sneak,
        Input::Sprint,
    ];
    fn index(self) -> usize {
        self as usize
    }
}

/// `BlockBreakHelper`: holds the attack key while a block is to be broken.
#[derive(Debug, Default)]
pub struct BlockBreakHelper {
    was_hitting: bool,
}

impl BlockBreakHelper {
    /// Whether to hold attack this tick.
    pub fn tick(&mut self, is_left_click: bool, looking_at_block: bool) -> bool {
        let hit = is_left_click && looking_at_block;
        self.was_hitting = hit;
        hit
    }
    /// `stopBreakingBlock`.
    pub fn stop_breaking_block(&mut self) {
        self.was_hitting = false;
    }
    pub fn is_hitting(&self) -> bool {
        self.was_hitting
    }
}

/// `BlockPlaceHelper`: one use click every `rightClickSpeed` ticks while placing is requested.
#[derive(Debug, Default)]
pub struct BlockPlaceHelper {
    right_click_timer: i32,
}

impl BlockPlaceHelper {
    /// Whether to click use this tick.
    pub fn tick(&mut self, right_click_requested: bool, looking_at_block: bool, hands_busy: bool, settings: &Settings) -> bool {
        if self.right_click_timer > 0 {
            self.right_click_timer -= 1;
            return false;
        }
        if !right_click_requested || hands_busy || !looking_at_block {
            return false;
        }
        // BASE_PLACE_DELAY is one tick.
        self.right_click_timer = settings.right_click_speed - 1;
        true
    }
}

/// What the input handler decided for this tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InputDecision {
    pub attack: bool,
    pub use_item: bool,
    /// Baritone owns the movement keys this tick.
    pub movement: Option<MovementKeys>,
}

/// `InputOverrideHandler`.
#[derive(Debug, Default)]
pub struct InputOverrideHandler {
    forced: [bool; 9],
    pub break_helper: BlockBreakHelper,
    pub place_helper: BlockPlaceHelper,
}

impl InputOverrideHandler {
    pub fn is_input_forced_down(&self, input: Input) -> bool {
        self.forced[input.index()]
    }
    pub fn set_input_force_state(&mut self, input: Input, forced: bool) {
        self.forced[input.index()] = forced;
    }
    pub fn clear_all_keys(&mut self) {
        self.forced = [false; 9];
    }

    /// `onTick` (IN): the break and place helpers, then whether Baritone owns the movement keys.
    /// `sprint` is the path executor's sprint wish while pathing (`onPlayerSprintState`).
    pub fn tick(
        &mut self,
        looking_at_block: bool,
        hands_busy: bool,
        pathing: bool,
        sprint: Option<bool>,
        settings: &Settings,
    ) -> InputDecision {
        if self.is_input_forced_down(Input::ClickLeft) {
            self.set_input_force_state(Input::ClickRight, false);
        }
        let attack = self
            .break_helper
            .tick(self.is_input_forced_down(Input::ClickLeft), looking_at_block);
        let use_item = self.place_helper.tick(
            self.is_input_forced_down(Input::ClickRight),
            looking_at_block,
            hands_busy,
            settings,
        );
        let in_control = [
            Input::MoveForward,
            Input::MoveBack,
            Input::MoveLeft,
            Input::MoveRight,
            Input::Sneak,
            Input::Jump,
        ]
        .into_iter()
        .any(|i| self.is_input_forced_down(i))
            || pathing;
        let movement = in_control.then(|| MovementKeys {
            forward: self.is_input_forced_down(Input::MoveForward),
            back: self.is_input_forced_down(Input::MoveBack),
            left: self.is_input_forced_down(Input::MoveLeft),
            right: self.is_input_forced_down(Input::MoveRight),
            jump: self.is_input_forced_down(Input::Jump),
            sneak: self.is_input_forced_down(Input::Sneak),
            sprint: sprint.unwrap_or(self.is_input_forced_down(Input::Sprint)),
        });
        InputDecision {
            attack,
            use_item,
            movement,
        }
    }
}
