//! Goals (`baritone.api.pathing.goals`): where a path should end, and a heuristic of how far a position is from
//! there, in ticks.

use super::costs;
use crate::math::{BlockPos, Direction, Vec3, mth};
use std::any::Any;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};

static COST_HEURISTIC: AtomicU64 = AtomicU64::new(0x400c_8106_24dd_2f1b); // 3.563, `costHeuristic`'s default
static AXIS_HEIGHT: AtomicI32 = AtomicI32::new(120);

/// Copies the two settings goals read (`costHeuristic`, `axisHeight`) where every thread sees them.
pub fn set_goal_settings(cost_heuristic: f64, axis_height: i32) {
    COST_HEURISTIC.store(cost_heuristic.to_bits(), Ordering::Relaxed);
    AXIS_HEIGHT.store(axis_height, Ordering::Relaxed);
}

#[inline]
fn cost_heuristic() -> f64 {
    f64::from_bits(COST_HEURISTIC.load(Ordering::Relaxed))
}

fn axis_height() -> i32 {
    AXIS_HEIGHT.load(Ordering::Relaxed)
}

/// `Goal`.
pub trait Goal: Send + Sync + fmt::Debug + fmt::Display {
    fn is_in_goal(&self, x: i32, y: i32, z: i32) -> bool;
    /// An estimate of the ticks from here to the goal.
    fn heuristic(&self, x: i32, y: i32, z: i32) -> f64;
    /// `heuristic()`: a value that is guaranteed to be inside the goal.
    fn heuristic_inside(&self) -> f64 {
        0.0
    }
    /// `IGoalRenderPos.getGoalPos`.
    fn render_pos(&self) -> Option<BlockPos> {
        None
    }
    fn as_any(&self) -> &dyn Any;
    /// `equals`.
    fn same_as(&self, other: &dyn Goal) -> bool;

    fn is_in_goal_pos(&self, pos: BlockPos) -> bool {
        self.is_in_goal(pos.x, pos.y, pos.z)
    }
    fn heuristic_pos(&self, pos: BlockPos) -> f64 {
        self.heuristic(pos.x, pos.y, pos.z)
    }
}

pub type GoalRef = Arc<dyn Goal>;

/// Whether two optional goals are equal (`Objects.equals`).
pub fn goals_equal(a: Option<&GoalRef>, b: Option<&GoalRef>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a.same_as(b.as_ref()),
        _ => false,
    }
}

macro_rules! goal_common {
    () => {
        fn as_any(&self) -> &dyn Any {
            self
        }
        fn same_as(&self, other: &dyn Goal) -> bool {
            other
                .as_any()
                .downcast_ref::<Self>()
                .is_some_and(|o| o == self)
        }
    };
}

fn censor(v: i32) -> String {
    if crate::settings::censor_coordinates() {
        "<censored>".into()
    } else {
        v.to_string()
    }
}

/// `GoalBlock`: stand at a block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GoalBlock {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl GoalBlock {
    pub fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }
    pub fn at(pos: BlockPos) -> Self {
        Self::new(pos.x, pos.y, pos.z)
    }
    /// `GoalBlock.calculate`: the Y part plus `GoalXZ.calculate`.
    pub fn calculate(x_diff: f64, y_diff: i32, z_diff: f64) -> f64 {
        GoalYLevel::calculate(0, y_diff) + GoalXZ::calculate(x_diff, z_diff)
    }
}

impl Goal for GoalBlock {
    fn is_in_goal(&self, x: i32, y: i32, z: i32) -> bool {
        x == self.x && y == self.y && z == self.z
    }
    fn heuristic(&self, x: i32, y: i32, z: i32) -> f64 {
        Self::calculate((x - self.x) as f64, y - self.y, (z - self.z) as f64)
    }
    fn render_pos(&self) -> Option<BlockPos> {
        Some(BlockPos::new(self.x, self.y, self.z))
    }
    goal_common!();
}

impl fmt::Display for GoalBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GoalBlock{{x={},y={},z={}}}", censor(self.x), censor(self.y), censor(self.z))
    }
}

/// `GoalXZ`: reach a column, at any height.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GoalXZ {
    pub x: i32,
    pub z: i32,
}

impl GoalXZ {
    pub fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }
    /// A mix of Pythagorean and Manhattan distance: walking forward 1 and right 2 really takes 1 + sqrt(2).
    pub fn calculate(x_diff: f64, z_diff: f64) -> f64 {
        let x = x_diff.abs();
        let z = z_diff.abs();
        let (straight, diagonal) = if x < z { (z - x, x) } else { (x - z, z) };
        (diagonal * std::f64::consts::SQRT_2 + straight) * cost_heuristic()
    }
    /// `fromDirection`: the column `distance` blocks along `yaw` from `origin`.
    pub fn from_direction(origin: Vec3, yaw: f32, distance: f64) -> Self {
        let theta = yaw.to_radians();
        let x = origin.x - mth::sin(theta) as f64 * distance;
        let z = origin.z + mth::cos(theta) as f64 * distance;
        Self::new(mth::floor(x), mth::floor(z))
    }
}

impl Goal for GoalXZ {
    fn is_in_goal(&self, x: i32, _y: i32, z: i32) -> bool {
        x == self.x && z == self.z
    }
    fn heuristic(&self, x: i32, _y: i32, z: i32) -> f64 {
        Self::calculate((x - self.x) as f64, (z - self.z) as f64)
    }
    goal_common!();
}

impl fmt::Display for GoalXZ {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GoalXZ{{x={},z={}}}", censor(self.x), censor(self.z))
    }
}

/// `GoalYLevel`: reach a height.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GoalYLevel {
    pub level: i32,
}

impl GoalYLevel {
    pub fn new(level: i32) -> Self {
        Self { level }
    }
    pub fn calculate(goal_y: i32, current_y: i32) -> f64 {
        if current_y > goal_y {
            // need to descend
            return costs::fall_n_blocks_cost(2) / 2.0 * (current_y - goal_y) as f64;
        }
        if current_y < goal_y {
            // need to ascend
            return (goal_y - current_y) as f64 * costs::jump_one_block_cost();
        }
        0.0
    }
}

impl Goal for GoalYLevel {
    fn is_in_goal(&self, _x: i32, y: i32, _z: i32) -> bool {
        y == self.level
    }
    fn heuristic(&self, _x: i32, y: i32, _z: i32) -> f64 {
        Self::calculate(self.level, y)
    }
    goal_common!();
}

impl fmt::Display for GoalYLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GoalYLevel{{y={}}}", censor(self.level))
    }
}

/// `GoalTwoBlocks`: have the feet or the head at a block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GoalTwoBlocks {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl GoalTwoBlocks {
    pub fn at(pos: BlockPos) -> Self {
        Self {
            x: pos.x,
            y: pos.y,
            z: pos.z,
        }
    }
}

impl Goal for GoalTwoBlocks {
    fn is_in_goal(&self, x: i32, y: i32, z: i32) -> bool {
        x == self.x && (y == self.y || y == self.y - 1) && z == self.z
    }
    fn heuristic(&self, x: i32, y: i32, z: i32) -> f64 {
        let y_diff = y - self.y;
        GoalBlock::calculate(
            (x - self.x) as f64,
            if y_diff < 0 { y_diff + 1 } else { y_diff },
            (z - self.z) as f64,
        )
    }
    fn render_pos(&self) -> Option<BlockPos> {
        Some(BlockPos::new(self.x, self.y, self.z))
    }
    goal_common!();
}

impl fmt::Display for GoalTwoBlocks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GoalTwoBlocks{{x={},y={},z={}}}", censor(self.x), censor(self.y), censor(self.z))
    }
}

/// `GoalGetToBlock`: stand next to a block, or on it, or under it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GoalGetToBlock {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl GoalGetToBlock {
    pub fn at(pos: BlockPos) -> Self {
        Self {
            x: pos.x,
            y: pos.y,
            z: pos.z,
        }
    }
}

impl Goal for GoalGetToBlock {
    fn is_in_goal(&self, x: i32, y: i32, z: i32) -> bool {
        let x_diff = x - self.x;
        let y_diff = y - self.y;
        let z_diff = z - self.z;
        x_diff.abs() + (if y_diff < 0 { y_diff + 1 } else { y_diff }).abs() + z_diff.abs() <= 1
    }
    fn heuristic(&self, x: i32, y: i32, z: i32) -> f64 {
        let y_diff = y - self.y;
        GoalBlock::calculate(
            (x - self.x) as f64,
            if y_diff < 0 { y_diff + 1 } else { y_diff },
            (z - self.z) as f64,
        )
    }
    fn render_pos(&self) -> Option<BlockPos> {
        Some(BlockPos::new(self.x, self.y, self.z))
    }
    goal_common!();
}

impl fmt::Display for GoalGetToBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GoalGetToBlock{{x={},y={},z={}}}", censor(self.x), censor(self.y), censor(self.z))
    }
}

/// `GoalNear`: within a distance of a block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GoalNear {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub range_sq: i32,
}

impl GoalNear {
    pub fn new(pos: BlockPos, range: i32) -> Self {
        Self {
            x: pos.x,
            y: pos.y,
            z: pos.z,
            range_sq: range * range,
        }
    }
}

/// The largest heuristic certain to be inside a goal, over a cube of positions around it (pull request #1978).
fn max_inside(points: impl Iterator<Item = (i32, i32, i32)>, goal: &dyn Goal) -> f64 {
    let mut maybe_inside: Vec<f64> = Vec::new();
    let mut min_outside = f64::INFINITY;
    for (x, y, z) in points {
        let h = goal.heuristic(x, y, z);
        if h < min_outside && goal.is_in_goal(x, y, z) {
            maybe_inside.push(h);
        } else {
            min_outside = min_outside.min(h);
        }
    }
    maybe_inside
        .into_iter()
        .filter(|h| *h < min_outside)
        .fold(f64::NEG_INFINITY, f64::max)
}

impl Goal for GoalNear {
    fn is_in_goal(&self, x: i32, y: i32, z: i32) -> bool {
        let (dx, dy, dz) = (x - self.x, y - self.y, z - self.z);
        dx * dx + dy * dy + dz * dz <= self.range_sq
    }
    fn heuristic(&self, x: i32, y: i32, z: i32) -> f64 {
        GoalBlock::calculate((x - self.x) as f64, y - self.y, (z - self.z) as f64)
    }
    fn heuristic_inside(&self) -> f64 {
        let range = (self.range_sq as f64).sqrt().ceil() as i32;
        let (cx, cy, cz) = (self.x, self.y, self.z);
        let points = (-range..=range).flat_map(move |dx| {
            (-range..=range)
                .flat_map(move |dy| (-range..=range).map(move |dz| (cx + dx, cy + dy, cz + dz)))
        });
        max_inside(points, self)
    }
    fn render_pos(&self) -> Option<BlockPos> {
        Some(BlockPos::new(self.x, self.y, self.z))
    }
    goal_common!();
}

impl fmt::Display for GoalNear {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "GoalNear{{x={}, y={}, z={}, rangeSq={}}}",
            censor(self.x),
            censor(self.y),
            censor(self.z),
            self.range_sq
        )
    }
}

/// `GoalComposite`: any of several goals.
#[derive(Debug, Clone)]
pub struct GoalComposite {
    pub goals: Vec<GoalRef>,
}

impl GoalComposite {
    pub fn new(goals: Vec<GoalRef>) -> Self {
        Self { goals }
    }
}

impl PartialEq for GoalComposite {
    fn eq(&self, o: &Self) -> bool {
        self.goals.len() == o.goals.len()
            && self
                .goals
                .iter()
                .zip(&o.goals)
                .all(|(a, b)| a.same_as(b.as_ref()))
    }
}

impl Goal for GoalComposite {
    fn is_in_goal(&self, x: i32, y: i32, z: i32) -> bool {
        self.goals.iter().any(|g| g.is_in_goal(x, y, z))
    }
    fn heuristic(&self, x: i32, y: i32, z: i32) -> f64 {
        self.goals
            .iter()
            .map(|g| g.heuristic(x, y, z))
            .fold(f64::MAX, f64::min)
    }
    fn heuristic_inside(&self) -> f64 {
        self.goals
            .iter()
            .map(|g| g.heuristic_inside())
            .fold(f64::MAX, f64::min)
    }
    goal_common!();
}

impl fmt::Display for GoalComposite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GoalComposite[")?;
        for (i, g) in self.goals.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{g}")?;
        }
        write!(f, "]")
    }
}

/// `GoalInverted`: get away from a goal.
#[derive(Debug, Clone)]
pub struct GoalInverted {
    pub origin: GoalRef,
}

impl PartialEq for GoalInverted {
    fn eq(&self, o: &Self) -> bool {
        self.origin.same_as(o.origin.as_ref())
    }
}

impl Goal for GoalInverted {
    fn is_in_goal(&self, _x: i32, _y: i32, _z: i32) -> bool {
        false
    }
    fn heuristic(&self, x: i32, y: i32, z: i32) -> f64 {
        -self.origin.heuristic(x, y, z)
    }
    fn heuristic_inside(&self) -> f64 {
        f64::NEG_INFINITY
    }
    goal_common!();
}

impl fmt::Display for GoalInverted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GoalInverted{{{}}}", self.origin)
    }
}

/// `GoalRunAway`: be at least a distance away (horizontally) from all of some positions.
#[derive(Debug, Clone, PartialEq)]
pub struct GoalRunAway {
    pub from: Vec<BlockPos>,
    pub distance_sq: i32,
    pub maintain_y: Option<i32>,
}

impl GoalRunAway {
    pub fn new(distance: f64, maintain_y: Option<i32>, from: Vec<BlockPos>) -> Self {
        assert!(!from.is_empty(), "Positions to run away from must not be empty");
        Self {
            from,
            distance_sq: (distance * distance) as i32,
            maintain_y,
        }
    }
}

impl Goal for GoalRunAway {
    fn is_in_goal(&self, x: i32, y: i32, z: i32) -> bool {
        if self.maintain_y.is_some_and(|m| m != y) {
            return false;
        }
        self.from.iter().all(|p| {
            let (dx, dz) = (x - p.x, z - p.z);
            dx * dx + dz * dz >= self.distance_sq
        })
    }
    fn heuristic(&self, x: i32, y: i32, z: i32) -> f64 {
        let mut min = self
            .from
            .iter()
            .map(|p| GoalXZ::calculate((p.x - x) as f64, (p.z - z) as f64))
            .fold(f64::MAX, f64::min);
        min = -min;
        if let Some(m) = self.maintain_y {
            min = min * 0.6 + GoalYLevel::calculate(m, y) * 1.5;
        }
        min
    }
    fn heuristic_inside(&self) -> f64 {
        let d = (self.distance_sq as f64).sqrt().ceil() as i32;
        let (mut min_x, mut min_y, mut min_z) = (i32::MAX, i32::MAX, i32::MAX);
        let (mut max_x, mut max_y, mut max_z) = (i32::MIN, i32::MIN, i32::MIN);
        for p in &self.from {
            min_x = min_x.min(p.x - d);
            min_y = min_y.min(p.y - d);
            min_z = min_z.min(p.z - d);
            // Baritone takes the max with the running minimum here; kept so the search bounds match.
            max_x = min_x.max(p.x + d);
            max_y = min_y.max(p.y + d);
            max_z = min_z.max(p.z + d);
        }
        let _ = (max_x, max_y, max_z);
        let points = (min_x..=max_x)
            .flat_map(move |x| (min_y..=max_y).flat_map(move |y| (min_z..=max_z).map(move |z| (x, y, z))));
        max_inside(points, self)
    }
    goal_common!();
}

impl fmt::Display for GoalRunAway {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.maintain_y {
            Some(y) => write!(f, "GoalRunAwayFromMaintainY y={}, {:?}", censor(y), self.from),
            None => write!(f, "GoalRunAwayFrom{:?}", self.from),
        }
    }
}

/// `GoalStrictDirection`: keep going one way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GoalStrictDirection {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub dx: i32,
    pub dz: i32,
}

impl GoalStrictDirection {
    pub fn new(origin: BlockPos, direction: Direction) -> Self {
        let [dx, _, dz] = direction.normal();
        assert!(dx != 0 || dz != 0, "{direction:?}");
        Self {
            x: origin.x,
            y: origin.y,
            z: origin.z,
            dx,
            dz,
        }
    }
}

impl Goal for GoalStrictDirection {
    fn is_in_goal(&self, _x: i32, _y: i32, _z: i32) -> bool {
        false
    }
    fn heuristic(&self, x: i32, y: i32, z: i32) -> f64 {
        let desired = (x - self.x) * self.dx + (z - self.z) * self.dz;
        let incorrect = ((x - self.x) * self.dz).abs() + ((z - self.z) * self.dx).abs();
        let vertical = (y - self.y).abs();
        -(desired as f64) * 100.0 + incorrect as f64 * 1000.0 + vertical as f64 * 1000.0
    }
    fn heuristic_inside(&self) -> f64 {
        f64::NEG_INFINITY
    }
    goal_common!();
}

impl fmt::Display for GoalStrictDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "GoalStrictDirection{{x={}, y={}, z={}, dx={}, dz={}}}",
            censor(self.x),
            censor(self.y),
            censor(self.z),
            censor(self.dx),
            censor(self.dz)
        )
    }
}

/// `GoalAxis`: reach an axis or diagonal of the world at `axisHeight`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GoalAxis;

impl Goal for GoalAxis {
    fn is_in_goal(&self, x: i32, y: i32, z: i32) -> bool {
        y == axis_height() && (x == 0 || z == 0 || x.abs() == z.abs())
    }
    fn heuristic(&self, x0: i32, y: i32, z0: i32) -> f64 {
        let x = x0.abs();
        let z = z0.abs();
        let shrt = x.min(z);
        let lng = x.max(z);
        let diff = lng - shrt;
        let flat = (x as f64)
            .min((z as f64).min(diff as f64 * std::f64::consts::FRAC_1_SQRT_2));
        flat * cost_heuristic() + GoalYLevel::calculate(axis_height(), y)
    }
    goal_common!();
}

impl fmt::Display for GoalAxis {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GoalAxis")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn goal_block_heuristic_prefers_closer() {
        let g = GoalBlock::new(10, 64, 10);
        assert!(g.heuristic(0, 64, 0) > g.heuristic(5, 64, 5));
        assert_eq!(g.heuristic(10, 64, 10), 0.0);
        assert!(g.is_in_goal(10, 64, 10));
        assert!(!g.is_in_goal(10, 65, 10));
    }

    #[test]
    fn get_to_block_includes_neighbours() {
        let g = GoalGetToBlock::at(BlockPos::new(0, 64, 0));
        assert!(g.is_in_goal(1, 64, 0));
        assert!(g.is_in_goal(0, 63, 0));
        assert!(g.is_in_goal(0, 65, 0));
        assert!(!g.is_in_goal(1, 65, 0));
    }

    #[test]
    fn composite_and_equality() {
        let a: GoalRef = Arc::new(GoalXZ::new(1, 2));
        let b: GoalRef = Arc::new(GoalXZ::new(1, 2));
        let c: GoalRef = Arc::new(GoalYLevel::new(3));
        assert!(a.same_as(b.as_ref()));
        assert!(!a.same_as(c.as_ref()));
        let comp = GoalComposite::new(vec![a.clone(), c.clone()]);
        assert!(comp.is_in_goal(1, 100, 2));
        assert!(comp.is_in_goal(50, 3, 50));
        assert_eq!(comp.heuristic(1, 100, 2), 0.0);
    }

    #[test]
    fn near_heuristic_inside_is_finite() {
        let g = GoalNear::new(BlockPos::new(0, 64, 0), 3);
        let h = g.heuristic_inside();
        assert!(h.is_finite() && h >= 0.0);
    }
}
