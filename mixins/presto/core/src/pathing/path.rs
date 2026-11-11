//! `IPath` and its kinds (`Path`, `CutoffPath`, `SplicedPath`): positions and the movements between them.

use super::context::CalculationContext;
use super::goals::GoalRef;
use super::movement::{Movement, Moves};
use crate::math::BlockPos;
use crate::settings::Settings;

#[derive(Clone, Debug)]
pub struct Path {
    positions: Vec<BlockPos>,
    movements: Vec<Movement>,
    pub goal: GoalRef,
    pub num_nodes: usize,
}

impl Path {
    /// A path made from the positions the search found and each step's cost by its nodes; the movements are
    /// built by [`Path::post_process`].
    pub(crate) fn from_positions(positions: Vec<BlockPos>, goal: GoalRef, num_nodes: usize) -> Self {
        Self {
            positions,
            movements: Vec::new(),
            goal,
            num_nodes,
        }
    }

    pub fn positions(&self) -> &[BlockPos] {
        &self.positions
    }
    pub fn movements(&self) -> &[Movement] {
        &self.movements
    }
    pub fn movements_mut(&mut self) -> &mut [Movement] {
        &mut self.movements
    }
    pub fn len(&self) -> usize {
        self.positions.len()
    }
    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }
    pub fn src(&self) -> BlockPos {
        self.positions[0]
    }
    pub fn dest(&self) -> BlockPos {
        *self.positions.last().expect("a path has positions")
    }

    /// `postProcess`: builds the movements by running each step's move again (`assembleMovements`). A step
    /// that has become impossible since cuts the path short there.
    pub(crate) fn post_process(mut self, node_costs: &[f64], ctx: &CalculationContext) -> Path {
        assert!(self.movements.is_empty(), "Path must not be verified twice");
        let mut failed_at = None;
        for i in 0..self.positions.len().saturating_sub(1) {
            let cost = node_costs[i + 1] - node_costs[i];
            match run_backwards(self.positions[i], self.positions[i + 1], cost, ctx) {
                Some(m) => self.movements.push(m),
                None => {
                    log::debug!(
                        "[Presto] Movement became impossible during calculation {:?} {:?}",
                        self.positions[i],
                        self.positions[i + 1]
                    );
                    failed_at = Some(i);
                    break;
                }
            }
        }
        for m in &mut self.movements {
            m.check_loaded_chunk(ctx);
        }
        if let Some(i) = failed_at {
            self.positions.truncate(i + 1);
        }
        self.sanity_check();
        self
    }

    /// `sanityCheck`: positions and movements line up.
    pub fn sanity_check(&self) {
        debug_assert!(!self.positions.is_empty(), "empty path");
        debug_assert_eq!(self.positions.len(), self.movements.len() + 1, "wrong size");
        for (i, m) in self.movements.iter().enumerate() {
            debug_assert_eq!(m.src, self.positions[i], "movement {i} doesn't start where it should");
            debug_assert_eq!(m.dest, self.positions[i + 1], "movement {i} doesn't end where it should");
        }
    }

    /// `CutoffPath(prev, first, last)`: positions `first..=last`.
    pub fn cutoff(&self, first: usize, last: usize) -> Path {
        let p = Path {
            positions: self.positions[first..=last].to_vec(),
            movements: self.movements[first..last].to_vec(),
            goal: self.goal.clone(),
            num_nodes: self.num_nodes,
        };
        p.sanity_check();
        p
    }

    /// `cutoffAtLoadedChunks`: stop where the client's loaded chunks end.
    pub fn cutoff_at_loaded_chunks(self, ctx: &CalculationContext, settings: &Settings) -> Path {
        if !settings.cutoff_at_load_boundary {
            return self;
        }
        for (i, p) in self.positions.iter().enumerate() {
            if !ctx.bsi.world_contains_loaded_chunk(p.x, p.z) {
                return self.cutoff(0, i);
            }
        }
        self
    }

    /// `staticCutoff`: a segment that doesn't reach the goal keeps only its first part, so the next segment
    /// is planned from where we'll be with fresher information.
    pub fn static_cutoff(self, destination: Option<&GoalRef>, settings: &Settings) -> Path {
        let min = settings.path_cutoff_minimum_length.max(0) as usize;
        if self.len() < min {
            return self;
        }
        match destination {
            None => return self,
            Some(g) if g.is_in_goal_pos(self.dest()) => return self,
            _ => {}
        }
        let factor = settings.path_cutoff_factor;
        let new_len = (((self.len() - min) as f64 * factor) as usize + min).saturating_sub(1);
        if new_len >= self.len() {
            return self;
        }
        self.cutoff(0, new_len)
    }

    /// `SplicedPath.trySplice`: `first` then `second`, joined where `first` ends and `second` starts.
    pub fn try_splice(first: &Path, second: &Path, allow_overlap_cutoff: bool) -> Option<Path> {
        if first.dest() != second.src() {
            return None;
        }
        let second_pos: rustc_hash::FxHashSet<BlockPos> = second.positions.iter().copied().collect();
        // Overlap at the very last position is fine (and required).
        let mut first_position_in_second = None;
        for i in 0..first.len() - 1 {
            if second_pos.contains(&first.positions[i]) {
                first_position_in_second = Some(i);
                break;
            }
        }
        let first_position_in_second = match first_position_in_second {
            Some(i) => {
                if !allow_overlap_cutoff {
                    return None;
                }
                i
            }
            None => first.len() - 1,
        };
        let position_in_second = second
            .positions
            .iter()
            .position(|p| *p == first.positions[first_position_in_second])?;
        if !allow_overlap_cutoff && position_in_second != 0 {
            panic!("Paths to be spliced are overlapping incorrectly");
        }
        let mut positions = first.positions[..=first_position_in_second].to_vec();
        let mut movements = first.movements[..first_position_in_second].to_vec();
        positions.extend_from_slice(&second.positions[position_in_second + 1..]);
        movements.extend_from_slice(&second.movements[position_in_second..second.len() - 1]);
        let p = Path {
            positions,
            movements,
            goal: first.goal.clone(),
            num_nodes: first.num_nodes + second.num_nodes,
        };
        p.sanity_check();
        Some(p)
    }

    /// The path's index of a position, if it is on it.
    pub fn index_of(&self, pos: BlockPos) -> Option<usize> {
        self.positions.iter().position(|p| *p == pos)
    }

    pub fn contains(&self, pos: BlockPos) -> bool {
        self.positions.contains(&pos)
    }
}

/// `Path.runBackwards`: the move from `src` that lands on `dest`, with the stricter of its cost now and the
/// cost the search saw (favoring can skew the latter).
fn run_backwards(src: BlockPos, dest: BlockPos, cost: f64, ctx: &CalculationContext) -> Option<Movement> {
    for mv in Moves::ALL {
        let mut m = mv.apply0(ctx, src);
        if m.dest == dest {
            let c = m.calculate_cost(ctx);
            m.override_cost(c.min(cost));
            return Some(m);
        }
    }
    None
}
