//! The A* search (`AStarPathFinder`, `AbstractNodeCostSearch`, `PathNode`, `BinaryHeapOpenSet`).
//!
//! Laid out for speed: nodes live in one `Vec` and link to their parent by index; positions map to nodes
//! through an open-addressed table keyed by the packed position (no collisions, a strong 64-bit mix); the
//! open set is a binary heap of `(combined cost, node)` pairs, so sifting compares costs without touching
//! the nodes. Block reads go through the context's flat section copies.

use super::context::CalculationContext;
use super::costs::COST_INF;
use super::goals::GoalRef;
use super::movement::{MoveResult, Moves};
use super::path::Path;
use crate::math::BlockPos;
use crate::world::WorldBorder;
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// `AbstractNodeCostSearch.COEFFICIENTS`: the weights the "best so far" is tracked under, for a fallback
/// path when the goal isn't reached in time.
pub const COEFFICIENTS: [f64; 7] = [1.5, 2.0, 2.5, 3.0, 4.0, 5.0, 10.0];
/// A fallback path must reach at least this far from the start.
pub const MIN_DIST_PATH: f64 = 5.0;
/// `MIN_IMPROVEMENT`: a cheaper way to a node must beat the old by this much (`minimumImprovementRepropagation`).
pub const MIN_IMPROVEMENT: f64 = 0.01;

const NO_NODE: u32 = u32::MAX;

/// `PathNode`.
#[derive(Clone, Copy, Debug)]
struct Node {
    x: i32,
    y: i32,
    z: i32,
    previous: u32,
    cost: f64,
    combined_cost: f64,
    estimated_cost_to_goal: f64,
    /// 1-based index in the open set; 0 when not in it.
    heap_position: u32,
}

/// A position packed into 64 bits (`BetterBlockPos.serializeToLong`), finalised with murmur3's mix so every
/// bit of x, y and z reaches the table index.
#[derive(Default, Clone, Copy)]
pub struct PosHasher(u64);

impl Hasher for PosHasher {
    #[inline]
    fn finish(&self) -> u64 {
        let mut k = self.0;
        k ^= k >> 33;
        k = k.wrapping_mul(0xff51afd7ed558ccd);
        k ^= k >> 33;
        k = k.wrapping_mul(0xc4ceb9fe1a85ec53);
        k ^= k >> 33;
        k
    }
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 = (self.0 << 8) | *b as u64;
        }
    }
    #[inline]
    fn write_u64(&mut self, i: u64) {
        self.0 = i;
    }
    #[inline]
    fn write_i64(&mut self, i: i64) {
        self.0 = i as u64;
    }
}

pub type PosMap<V> = HashMap<u64, V, BuildHasherDefault<PosHasher>>;

#[inline]
fn key(x: i32, y: i32, z: i32) -> u64 {
    BlockPos::new(x, y, z).to_long() as u64
}

/// `BinaryHeapOpenSet`, holding each node's combined cost beside its index.
struct OpenSet {
    /// 1-based: slot 0 is unused.
    heap: Vec<(f64, u32)>,
}

impl OpenSet {
    fn new() -> Self {
        let mut heap = Vec::with_capacity(1024);
        heap.push((0.0, NO_NODE));
        Self { heap }
    }

    #[inline]
    fn len(&self) -> usize {
        self.heap.len() - 1
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.heap.len() == 1
    }

    #[inline]
    fn insert(&mut self, nodes: &mut [Node], n: u32) {
        let pos = self.heap.len() as u32;
        self.heap.push((nodes[n as usize].combined_cost, n));
        nodes[n as usize].heap_position = pos;
        self.sift_up(nodes, pos);
    }

    /// `update`: the node's cost went down; move it up.
    #[inline]
    fn update(&mut self, nodes: &mut [Node], n: u32) {
        let pos = nodes[n as usize].heap_position;
        self.heap[pos as usize].0 = nodes[n as usize].combined_cost;
        self.sift_up(nodes, pos);
    }

    #[inline]
    fn sift_up(&mut self, nodes: &mut [Node], mut index: u32) {
        let entry = self.heap[index as usize];
        while index > 1 {
            let parent = index >> 1;
            let p = self.heap[parent as usize];
            if p.0 <= entry.0 {
                break;
            }
            self.heap[index as usize] = p;
            nodes[p.1 as usize].heap_position = index;
            index = parent;
        }
        self.heap[index as usize] = entry;
        nodes[entry.1 as usize].heap_position = index;
    }

    #[inline]
    fn remove_lowest(&mut self, nodes: &mut [Node]) -> u32 {
        let result = self.heap[1].1;
        nodes[result as usize].heap_position = 0;
        let last = self.heap.pop().expect("open set is not empty");
        let size = self.heap.len() - 1;
        if size == 0 {
            return result;
        }
        // Sift the last entry down from the root.
        let mut index = 1usize;
        loop {
            let mut child = index << 1;
            if child > size {
                break;
            }
            if child < size && self.heap[child + 1].0 < self.heap[child].0 {
                child += 1;
            }
            if last.0 <= self.heap[child].0 {
                break;
            }
            let c = self.heap[child];
            self.heap[index] = c;
            nodes[c.1 as usize].heap_position = index as u32;
            index = child;
        }
        self.heap[index] = last;
        nodes[last.1 as usize].heap_position = index as u32;
        result
    }
}

/// `Favoring`: multipliers on the cost of reaching positions: below 1 along the previous path
/// (`backtrackCostFavoringCoefficient`), above 1 near mobs and spawners (`Avoidance`).
#[derive(Clone, Debug, Default)]
pub struct Favoring {
    favorings: PosMap<f64>,
}

impl Favoring {
    pub fn new(previous: Option<&[BlockPos]>, coefficient: f64) -> Self {
        let mut f = Favoring::default();
        if coefficient != 1.0
            && let Some(prev) = previous
        {
            for p in prev {
                f.favorings.insert(p.to_long() as u64, coefficient);
            }
        }
        f
    }

    /// `Avoidance.applySpherical`.
    pub fn apply_spherical(&mut self, center: BlockPos, coefficient: f64, radius: i32) {
        for x in -radius..=radius {
            for y in -radius..=radius {
                for z in -radius..=radius {
                    if x * x + y * y + z * z <= radius * radius {
                        let k = key(center.x + x, center.y + y, center.z + z);
                        let v = self.favorings.entry(k).or_insert(1.0);
                        *v *= coefficient;
                    }
                }
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.favorings.is_empty()
    }

    #[inline]
    pub fn calculate(&self, k: u64) -> f64 {
        self.favorings.get(&k).copied().unwrap_or(1.0)
    }
}

/// `PathCalculationResult.Type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResultType {
    SuccessToGoal,
    SuccessSegment,
    Failure,
    Cancellation,
    Exception,
}

pub struct PathCalculationResult {
    pub kind: ResultType,
    pub path: Option<Path>,
}

/// What a running search shares with the game thread: its cancel flag, and the paths it would take now,
/// refreshed while it runs (for rendering and for `PathingBehavior`'s checks).
#[derive(Default)]
pub struct SearchShared {
    cancel: AtomicBool,
    finished: AtomicBool,
    progress: Mutex<SearchProgress>,
}

#[derive(Clone, Default)]
pub struct SearchProgress {
    /// `bestPathSoFar`.
    pub best_so_far: Option<Arc<Vec<BlockPos>>>,
    /// `pathToMostRecentNodeConsidered`.
    pub most_recent: Option<Arc<Vec<BlockPos>>>,
    pub nodes_considered: usize,
}

impl SearchShared {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
    pub fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Acquire)
    }
    pub fn progress(&self) -> SearchProgress {
        self.progress
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

/// Timeouts and the `slowPath` debugging delay.
#[derive(Clone, Copy, Debug)]
pub struct SearchLimits {
    pub primary_timeout_ms: u64,
    pub failure_timeout_ms: u64,
    pub slow_path: bool,
    pub slow_path_delay_ms: u64,
    pub slow_path_timeout_ms: u64,
    pub max_chunk_border_fetch: i32,
    pub minimum_improvement_repropagation: bool,
    pub map_default_size: usize,
}

/// `AStarPathFinder`.
pub struct AStarPathFinder {
    real_start: BlockPos,
    start: BlockPos,
    goal: GoalRef,
    ctx: CalculationContext,
    favoring: Favoring,
    nodes: Vec<Node>,
    map: PosMap<u32>,
    shared: Arc<SearchShared>,
}

impl AStarPathFinder {
    pub fn new(
        real_start: BlockPos,
        start: BlockPos,
        goal: GoalRef,
        favoring: Favoring,
        ctx: CalculationContext,
    ) -> Self {
        Self {
            real_start,
            start,
            goal,
            ctx,
            favoring,
            nodes: Vec::new(),
            map: PosMap::default(),
            shared: Arc::new(SearchShared::default()),
        }
    }

    pub fn shared(&self) -> Arc<SearchShared> {
        self.shared.clone()
    }

    pub fn goal(&self) -> &GoalRef {
        &self.goal
    }

    pub fn start(&self) -> BlockPos {
        self.start
    }

    #[inline]
    fn node_at(&mut self, x: i32, y: i32, z: i32) -> u32 {
        let k = key(x, y, z);
        if let Some(&i) = self.map.get(&k) {
            return i;
        }
        let estimated = self.goal.heuristic(x, y, z);
        assert!(
            !estimated.is_nan(),
            "{} calculated implausible heuristic NaN at {x} {y} {z}",
            self.goal
        );
        let i = self.nodes.len() as u32;
        self.nodes.push(Node {
            x,
            y,
            z,
            previous: NO_NODE,
            cost: COST_INF,
            combined_cost: 0.0,
            estimated_cost_to_goal: estimated,
            heap_position: 0,
        });
        self.map.insert(k, i);
        i
    }

    #[inline]
    fn dist_from_start_sq(&self, n: &Node) -> f64 {
        let dx = (n.x - self.start.x) as f64;
        let dy = (n.y - self.start.y) as f64;
        let dz = (n.z - self.start.z) as f64;
        dx * dx + dy * dy + dz * dz
    }

    fn chain(&self, end: u32) -> (Vec<BlockPos>, Vec<f64>) {
        let mut positions = Vec::new();
        let mut costs = Vec::new();
        let mut cur = end;
        while cur != NO_NODE {
            let n = &self.nodes[cur as usize];
            positions.push(BlockPos::new(n.x, n.y, n.z));
            costs.push(n.cost);
            cur = n.previous;
        }
        let start_node = BlockPos::new(self.start.x, self.start.y, self.start.z);
        // A* gave no movements and the player isn't where it started: add the real start, so a movement
        // to the one position exists (PathingBehavior#createPathfinder, Baritone PR #4519).
        if positions.len() == 1 && self.real_start != start_node {
            positions.push(self.real_start);
            costs.push(0.0);
        }
        positions.reverse();
        costs.reverse();
        (positions, costs)
    }

    fn positions_to(&self, end: u32) -> Vec<BlockPos> {
        self.chain(end).0
    }

    /// `bestSoFar`: the first coefficient's best node that got far enough from the start.
    fn best_so_far(&self, best: &[u32; 7]) -> Option<u32> {
        for b in best {
            if *b == NO_NODE {
                continue;
            }
            if self.dist_from_start_sq(&self.nodes[*b as usize]) > MIN_DIST_PATH * MIN_DIST_PATH {
                return Some(*b);
            }
        }
        None
    }

    fn publish(&self, best: &[u32; 7], recent: u32, num_nodes: usize) {
        let best_so_far = self.best_so_far(best).map(|b| Arc::new(self.positions_to(b)));
        let most_recent = (recent != NO_NODE).then(|| Arc::new(self.positions_to(recent)));
        let mut p = self.shared.progress.lock().unwrap_or_else(|e| e.into_inner());
        p.best_so_far = best_so_far;
        p.most_recent = most_recent;
        p.nodes_considered = num_nodes;
    }

    /// `calculate`: runs the search to the end, on the calling thread.
    pub fn calculate(mut self, limits: SearchLimits) -> PathCalculationResult {
        let shared = self.shared.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.calculate0(limits)));
        shared.finished.store(true, Ordering::Release);
        match result {
            Ok(r) => r,
            Err(e) => {
                let msg = e
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_default();
                log::error!("[Baritone] Pathing exception: {msg}");
                PathCalculationResult {
                    kind: ResultType::Exception,
                    path: None,
                }
            }
        }
    }

    fn calculate0(&mut self, limits: SearchLimits) -> PathCalculationResult {
        let path = self.search(limits);
        if self.shared.is_cancelled() {
            return PathCalculationResult {
                kind: ResultType::Cancellation,
                path: None,
            };
        }
        let Some((positions, costs, num_nodes)) = path else {
            return PathCalculationResult {
                kind: ResultType::Failure,
                path: None,
            };
        };
        let path = Path::from_positions(positions, self.goal.clone(), num_nodes).post_process(&costs, &self.ctx);
        let settings = self.ctx.settings.clone();
        let previous = path.len();
        let path = path.cutoff_at_loaded_chunks(&self.ctx, &settings);
        if path.len() < previous {
            log::debug!(
                "[Baritone] Cutting off path at edge of loaded chunks, length decreased by {}",
                previous - path.len()
            );
        }
        let previous = path.len();
        let path = path.static_cutoff(Some(&self.goal), &settings);
        if path.len() < previous {
            log::debug!("[Baritone] Static cutoff {previous} to {}", path.len());
        }
        let kind = if self.goal.is_in_goal_pos(path.dest()) {
            ResultType::SuccessToGoal
        } else {
            ResultType::SuccessSegment
        };
        PathCalculationResult {
            kind,
            path: Some(path),
        }
    }

    /// `calculate0`: the A* itself. Returns the path's positions, the cost at each, and the nodes expanded.
    fn search(&mut self, limits: SearchLimits) -> Option<(Vec<BlockPos>, Vec<f64>, usize)> {
        let min_y = self.ctx.min_y;
        let height = self.bsi_height();
        self.map.reserve(limits.map_default_size.max(1024));
        self.nodes.reserve(limits.map_default_size.max(1024));
        let start = self.node_at(self.start.x, self.start.y, self.start.z);
        {
            let s = &mut self.nodes[start as usize];
            s.cost = 0.0;
            s.combined_cost = s.estimated_cost_to_goal;
        }
        let mut open = OpenSet::new();
        open.insert(&mut self.nodes, start);
        let start_estimate = self.nodes[start as usize].estimated_cost_to_goal;
        let mut best_heuristic = [start_estimate; 7];
        let mut best = [start; 7];
        let mut res = MoveResult::new();
        let border: WorldBorder = self.ctx.world_border;
        let started = Instant::now();
        let (primary, failure) = if limits.slow_path {
            log::debug!(
                "[Baritone] slowPath is on, path timeout will be {}ms instead of {}ms",
                limits.slow_path_timeout_ms,
                limits.primary_timeout_ms
            );
            (limits.slow_path_timeout_ms, limits.slow_path_timeout_ms)
        } else {
            (limits.primary_timeout_ms, limits.failure_timeout_ms)
        };
        let primary_deadline = started + Duration::from_millis(primary);
        let failure_deadline = started + Duration::from_millis(failure);
        let mut last_publish = started;
        let mut failing = true;
        let mut num_nodes = 0usize;
        let mut num_movements_considered = 0usize;
        let mut num_empty_chunk = 0i32;
        let is_favoring = !self.favoring.is_empty();
        let minimum_improvement = if limits.minimum_improvement_repropagation {
            MIN_IMPROVEMENT
        } else {
            0.0
        };
        let mut most_recent = NO_NODE;
        while !open.is_empty()
            && num_empty_chunk < limits.max_chunk_border_fetch
            && !self.shared.is_cancelled()
        {
            if num_nodes & 63 == 0 {
                // A clock read every 64 nodes.
                let now = Instant::now();
                if now >= failure_deadline || (!failing && now >= primary_deadline) {
                    break;
                }
                if now.duration_since(last_publish) >= Duration::from_millis(50) {
                    last_publish = now;
                    self.publish(&best, most_recent, num_nodes);
                }
            }
            if limits.slow_path {
                std::thread::sleep(Duration::from_millis(limits.slow_path_delay_ms));
            }
            let current = open.remove_lowest(&mut self.nodes);
            most_recent = current;
            num_nodes += 1;
            let Node {
                x: cx,
                y: cy,
                z: cz,
                cost: current_cost,
                ..
            } = self.nodes[current as usize];
            if self.goal.is_in_goal(cx, cy, cz) {
                log::debug!(
                    "[Baritone] Took {}ms, {num_movements_considered} movements considered",
                    started.elapsed().as_millis()
                );
                self.publish(&best, most_recent, num_nodes);
                let (positions, costs) = self.chain(current);
                return Some((positions, costs, num_nodes));
            }
            for mv in Moves::ALL {
                let (dx, dy, dz) = mv.offset();
                let new_x = cx + dx;
                let new_z = cz + dz;
                let dynamic_xz = mv.dynamic_xz();
                if ((new_x >> 4) != (cx >> 4) || (new_z >> 4) != (cz >> 4)) && !self.ctx.is_loaded(new_x, new_z) {
                    // Only check loading when the move leaves the start's chunk.
                    if !dynamic_xz {
                        // Only count it when the move would certainly have left the loaded area.
                        num_empty_chunk += 1;
                    }
                    continue;
                }
                if !dynamic_xz && !border.entirely_contains(new_x, new_z) {
                    continue;
                }
                let new_y_nominal = cy + dy;
                if new_y_nominal > height || new_y_nominal < min_y {
                    continue;
                }
                res.reset();
                mv.apply(&self.ctx, cx, cy, cz, &mut res);
                num_movements_considered += 1;
                let mut action_cost = res.cost;
                if action_cost >= COST_INF {
                    continue;
                }
                assert!(
                    action_cost > 0.0 && !action_cost.is_nan(),
                    "{mv:?} from {cx} {cy} {cz} calculated implausible cost {action_cost}"
                );
                // Some moves return COST_INF without setting where they end, so check after.
                if dynamic_xz && !border.entirely_contains(res.x, res.z) {
                    continue; // issue #218
                }
                debug_assert!(
                    dynamic_xz || (res.x == new_x && res.z == new_z),
                    "{mv:?} from {cx} {cy} {cz} ended at x z {} {} instead of {new_x} {new_z}",
                    res.x,
                    res.z
                );
                debug_assert!(
                    mv.dynamic_y() || res.y == new_y_nominal,
                    "{mv:?} from {cx} {cy} {cz} ended at y {} instead of {new_y_nominal}",
                    res.y
                );
                if is_favoring {
                    // Issue #18.
                    action_cost *= self.favoring.calculate(key(res.x, res.y, res.z));
                }
                let neighbor = self.node_at(res.x, res.y, res.z);
                let tentative = current_cost + action_cost;
                let n = &mut self.nodes[neighbor as usize];
                if n.cost - tentative > minimum_improvement {
                    n.previous = current;
                    n.cost = tentative;
                    n.combined_cost = tentative + n.estimated_cost_to_goal;
                    let estimated = n.estimated_cost_to_goal;
                    let open_already = n.heap_position != 0;
                    let dist_sq = {
                        let (nx, ny, nz) = (n.x, n.y, n.z);
                        let dx = (nx - self.start.x) as f64;
                        let dy = (ny - self.start.y) as f64;
                        let dz = (nz - self.start.z) as f64;
                        dx * dx + dy * dy + dz * dz
                    };
                    if open_already {
                        open.update(&mut self.nodes, neighbor);
                    } else {
                        open.insert(&mut self.nodes, neighbor);
                    }
                    for (i, coefficient) in COEFFICIENTS.iter().enumerate() {
                        let heuristic = estimated + tentative / coefficient;
                        if best_heuristic[i] - heuristic > minimum_improvement {
                            best_heuristic[i] = heuristic;
                            best[i] = neighbor;
                            if failing && dist_sq > MIN_DIST_PATH * MIN_DIST_PATH {
                                failing = false;
                            }
                        }
                    }
                }
            }
        }
        if self.shared.is_cancelled() {
            return None;
        }
        let elapsed = started.elapsed().as_secs_f64().max(1e-9);
        log::debug!(
            "[Baritone] {num_movements_considered} movements considered, open set {}, node map {}, {} nodes per second",
            open.len(),
            self.map.len(),
            (num_nodes as f64 / elapsed) as u64
        );
        self.publish(&best, most_recent, num_nodes);
        match self.best_so_far(&best) {
            Some(b) => {
                log::debug!(
                    "[Baritone] Took {}ms, {num_movements_considered} movements considered",
                    started.elapsed().as_millis()
                );
                let (positions, costs) = self.chain(b);
                Some((positions, costs, num_nodes))
            }
            None => {
                let best_dist = best
                    .iter()
                    .filter(|b| **b != NO_NODE)
                    .map(|b| self.dist_from_start_sq(&self.nodes[*b as usize]))
                    .fold(0.0, f64::max);
                log::debug!(
                    "[Baritone] Even with a cost coefficient of {}, I couldn't get more than {} blocks. No path found =(",
                    COEFFICIENTS[COEFFICIENTS.len() - 1],
                    best_dist.sqrt()
                );
                None
            }
        }
    }

    /// `world.dimensionType().height()` as the search's upper bound (Baritone compares the next y with the
    /// height itself, not the top).
    fn bsi_height(&self) -> i32 {
        self.ctx.bsi.dimension.height
    }
}
