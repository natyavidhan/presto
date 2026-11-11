//! `InputOverrideHandler`, `BlockBreakHelper` and `BlockPlaceHelper`: the keys Presto holds for the player.
//!
//! The waits between blocks are settings, not fixed: `blockBreakSpeed` (ticks from a block breaking to hitting
//! the next), `rightClickSpeed` (ticks between placements) and `randomActionDelayTicks` (a random extra wait
//! on both). Their defaults here are no wait at all.

use super::look::ForkableRandom;
use crate::math::BlockPos;
use crate::player::MovementKeys;
use crate::settings::Settings;
use crate::world::BlockId;

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

fn random_extra(rng: &mut ForkableRandom, max: i32) -> i32 {
    if max <= 0 {
        0
    } else {
        (rng.next_double() * (max + 1) as f64) as i32
    }
}

/// `BlockBreakHelper`: holds the attack key on the block under the crosshair, and after a block breaks,
/// waits `blockBreakSpeed - 1` ticks (plus up to `randomActionDelayTicks`) before hitting the next.
#[derive(Debug, Default)]
pub struct BlockBreakHelper {
    was_hitting: bool,
    /// `breakDelayTimer`.
    cooldown: i32,
    /// The block being hit, which block it was, and for how many ticks.
    target: Option<(BlockPos, BlockId)>,
    ticks_on_target: i32,
}

impl BlockBreakHelper {
    /// Whether to hold attack this tick. `aimed` is the block the crosshair will be on once this tick's turn
    /// is applied; `block_at` says which block a position holds now.
    pub fn tick(
        &mut self,
        is_left_click: bool,
        aimed: Option<BlockPos>,
        block_at: &dyn Fn(BlockPos) -> BlockId,
        settings: &Settings,
        rng: &mut ForkableRandom,
    ) -> bool {
        // The block we were hitting is gone (`hasBrokenBlock` after a hit).
        if let Some((pos, block)) = self.target
            && self.was_hitting
            && block_at(pos) != block
        {
            let instant = self.ticks_on_target <= 1;
            self.target = None;
            // As in Baritone, the break delay applies only to blocks that took more than a tick.
            let base = if instant { 0 } else { (settings.block_break_speed - 1).max(0) };
            self.cooldown = base + random_extra(rng, settings.random_action_delay_ticks);
        }
        if self.cooldown > 0 {
            self.cooldown -= 1;
            self.was_hitting = false;
            return false;
        }
        let hit = is_left_click && aimed.is_some();
        match aimed.filter(|_| hit) {
            Some(pos) => {
                if self.target.map(|t| t.0) != Some(pos) {
                    self.target = Some((pos, block_at(pos)));
                    self.ticks_on_target = 0;
                }
                self.ticks_on_target += 1;
            }
            None => self.target = None,
        }
        self.was_hitting = hit;
        hit
    }

    /// `stopBreakingBlock`.
    pub fn stop_breaking_block(&mut self) {
        self.was_hitting = false;
        self.target = None;
    }

    pub fn is_hitting(&self) -> bool {
        self.was_hitting
    }
}

/// `BlockPlaceHelper`: one use click every `rightClickSpeed` ticks (plus up to `randomActionDelayTicks`)
/// while placing is requested.
#[derive(Debug, Default)]
pub struct BlockPlaceHelper {
    right_click_timer: i32,
}

impl BlockPlaceHelper {
    /// Whether to click use this tick.
    pub fn tick(
        &mut self,
        right_click_requested: bool,
        aimed_at_block: bool,
        hands_busy: bool,
        settings: &Settings,
        rng: &mut ForkableRandom,
    ) -> bool {
        if self.right_click_timer > 0 {
            self.right_click_timer -= 1;
            return false;
        }
        if !right_click_requested || hands_busy || !aimed_at_block {
            return false;
        }
        // BASE_PLACE_DELAY is one tick.
        self.right_click_timer =
            (settings.right_click_speed - 1).max(0) + random_extra(rng, settings.random_action_delay_ticks);
        true
    }
}

/// What the input handler decided for this tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InputDecision {
    pub attack: bool,
    pub use_item: bool,
    /// Presto owns the movement keys this tick.
    pub movement: Option<MovementKeys>,
}

/// `InputOverrideHandler`.
#[derive(Debug)]
pub struct InputOverrideHandler {
    forced: [bool; 9],
    pub break_helper: BlockBreakHelper,
    pub place_helper: BlockPlaceHelper,
    rng: ForkableRandom,
}

impl Default for InputOverrideHandler {
    fn default() -> Self {
        Self {
            forced: [false; 9],
            break_helper: BlockBreakHelper::default(),
            place_helper: BlockPlaceHelper::default(),
            rng: ForkableRandom::from_time(),
        }
    }
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

    /// `onTick` (IN): the break and place helpers, then whether Presto owns the movement keys.
    /// `aimed` is the block the crosshair will be on after this tick's turn. `sprint` is the path
    /// executor's sprint wish while pathing (`onPlayerSprintState`).
    pub fn tick(
        &mut self,
        aimed: Option<BlockPos>,
        block_at: &dyn Fn(BlockPos) -> BlockId,
        hands_busy: bool,
        pathing: bool,
        sprint: Option<bool>,
        settings: &Settings,
    ) -> InputDecision {
        if self.is_input_forced_down(Input::ClickLeft) {
            self.set_input_force_state(Input::ClickRight, false);
        }
        let attack = self.break_helper.tick(
            self.is_input_forced_down(Input::ClickLeft),
            aimed,
            block_at,
            settings,
            &mut self.rng,
        );
        let use_item = self.place_helper.tick(
            self.is_input_forced_down(Input::ClickRight),
            aimed.is_some(),
            hands_busy,
            settings,
            &mut self.rng,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breaks_back_to_back_with_fast_defaults() {
        let settings = Settings::default();
        let mut h = BlockBreakHelper::default();
        let mut rng = ForkableRandom::new(1);
        let stone = BlockId(1);
        let air = BlockId(0);
        let a = BlockPos::new(0, 0, 0);
        let b = BlockPos::new(1, 0, 0);
        let mut broken = false;
        let world = |p: BlockPos, broken: bool| if p == a && broken { air } else { stone };
        // Three ticks of hitting `a`, then it breaks, then `b` is hit on the very next tick.
        for _ in 0..3 {
            assert!(h.tick(true, Some(a), &|p| world(p, broken), &settings, &mut rng));
        }
        broken = true;
        assert!(h.tick(true, Some(b), &|p| world(p, broken), &settings, &mut rng));
    }

    #[test]
    fn honours_block_break_speed() {
        let settings = Settings {
            block_break_speed: 6,
            ..Settings::default()
        };
        let mut h = BlockBreakHelper::default();
        let mut rng = ForkableRandom::new(1);
        let (stone, air) = (BlockId(1), BlockId(0));
        let a = BlockPos::new(0, 0, 0);
        let b = BlockPos::new(1, 0, 0);
        for _ in 0..3 {
            assert!(h.tick(true, Some(a), &|_| stone, &settings, &mut rng));
        }
        // `a` broke: five idle ticks, as Java Baritone's default does.
        let world = |p: BlockPos| if p == a { air } else { stone };
        for _ in 0..5 {
            assert!(!h.tick(true, Some(b), &world, &settings, &mut rng));
        }
        assert!(h.tick(true, Some(b), &world, &settings, &mut rng));
    }

    #[test]
    fn places_every_tick_by_default() {
        let settings = Settings::default();
        let mut p = BlockPlaceHelper::default();
        let mut rng = ForkableRandom::new(1);
        for _ in 0..5 {
            assert!(p.tick(true, true, false, &settings, &mut rng));
        }
        let vanilla = Settings {
            right_click_speed: 4,
            ..Settings::default()
        };
        let mut p = BlockPlaceHelper::default();
        let clicks = (0..12).filter(|_| p.tick(true, true, false, &vanilla, &mut rng)).count();
        assert_eq!(clicks, 3);
    }
}
