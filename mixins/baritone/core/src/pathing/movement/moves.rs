//! `Moves`: every move the search tries from a node, with its offset and its cost function.

use super::{Movement, ascend, descend, diagonal, downward, parkour, pillar, traverse};
use crate::math::{BlockPos, Direction};
use crate::pathing::context::CalculationContext;
use crate::pathing::costs::COST_INF;

/// `MutableMoveResult`: where a move ends and what it costs (`COST_INF` when impossible).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoveResult {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub cost: f64,
}

impl MoveResult {
    pub const fn new() -> Self {
        Self {
            x: 0,
            y: 0,
            z: 0,
            cost: COST_INF,
        }
    }
    #[inline]
    pub fn reset(&mut self) {
        *self = Self::new();
    }
}

impl Default for MoveResult {
    fn default() -> Self {
        Self::new()
    }
}

/// `Moves`, in Baritone's order (the search tries them in this order, which breaks cost ties).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Moves {
    Downward,
    Pillar,
    TraverseNorth,
    TraverseSouth,
    TraverseEast,
    TraverseWest,
    AscendNorth,
    AscendSouth,
    AscendEast,
    AscendWest,
    DescendEast,
    DescendWest,
    DescendNorth,
    DescendSouth,
    DiagonalNortheast,
    DiagonalNorthwest,
    DiagonalSoutheast,
    DiagonalSouthwest,
    ParkourNorth,
    ParkourSouth,
    ParkourEast,
    ParkourWest,
}

impl Moves {
    pub const ALL: [Moves; 22] = [
        Moves::Downward,
        Moves::Pillar,
        Moves::TraverseNorth,
        Moves::TraverseSouth,
        Moves::TraverseEast,
        Moves::TraverseWest,
        Moves::AscendNorth,
        Moves::AscendSouth,
        Moves::AscendEast,
        Moves::AscendWest,
        Moves::DescendEast,
        Moves::DescendWest,
        Moves::DescendNorth,
        Moves::DescendSouth,
        Moves::DiagonalNortheast,
        Moves::DiagonalNorthwest,
        Moves::DiagonalSoutheast,
        Moves::DiagonalSouthwest,
        Moves::ParkourNorth,
        Moves::ParkourSouth,
        Moves::ParkourEast,
        Moves::ParkourWest,
    ];

    /// `xOffset, yOffset, zOffset`: the nominal offset (the real one of a dynamic move comes from `apply`).
    #[inline]
    pub const fn offset(self) -> (i32, i32, i32) {
        match self {
            Moves::Downward => (0, -1, 0),
            Moves::Pillar => (0, 1, 0),
            Moves::TraverseNorth => (0, 0, -1),
            Moves::TraverseSouth => (0, 0, 1),
            Moves::TraverseEast => (1, 0, 0),
            Moves::TraverseWest => (-1, 0, 0),
            Moves::AscendNorth => (0, 1, -1),
            Moves::AscendSouth => (0, 1, 1),
            Moves::AscendEast => (1, 1, 0),
            Moves::AscendWest => (-1, 1, 0),
            Moves::DescendEast => (1, -1, 0),
            Moves::DescendWest => (-1, -1, 0),
            Moves::DescendNorth => (0, -1, -1),
            Moves::DescendSouth => (0, -1, 1),
            Moves::DiagonalNortheast => (1, 0, -1),
            Moves::DiagonalNorthwest => (-1, 0, -1),
            Moves::DiagonalSoutheast => (1, 0, 1),
            Moves::DiagonalSouthwest => (-1, 0, 1),
            Moves::ParkourNorth => (0, 0, -4),
            Moves::ParkourSouth => (0, 0, 4),
            Moves::ParkourEast => (4, 0, 0),
            Moves::ParkourWest => (-4, 0, 0),
        }
    }

    /// `dynamicXZ`: where it ends sideways is decided by `apply` (parkour).
    #[inline]
    pub const fn dynamic_xz(self) -> bool {
        matches!(
            self,
            Moves::ParkourNorth | Moves::ParkourSouth | Moves::ParkourEast | Moves::ParkourWest
        )
    }

    /// `dynamicY`: where it ends vertically is decided by `apply` (descend/fall, diagonal, parkour).
    #[inline]
    pub const fn dynamic_y(self) -> bool {
        !matches!(
            self,
            Moves::Downward
                | Moves::Pillar
                | Moves::TraverseNorth
                | Moves::TraverseSouth
                | Moves::TraverseEast
                | Moves::TraverseWest
                | Moves::AscendNorth
                | Moves::AscendSouth
                | Moves::AscendEast
                | Moves::AscendWest
        )
    }

    /// `apply`: where the move from `x, y, z` ends and what it costs.
    #[inline]
    pub fn apply(self, ctx: &CalculationContext, x: i32, y: i32, z: i32, res: &mut MoveResult) {
        let fixed = |res: &mut MoveResult, c: f64| {
            let (dx, dy, dz) = self.offset();
            res.x = x + dx;
            res.y = y + dy;
            res.z = z + dz;
            res.cost = c;
        };
        match self {
            Moves::Downward => fixed(res, downward::cost(ctx, x, y, z)),
            Moves::Pillar => fixed(res, pillar::cost(ctx, x, y, z)),
            Moves::TraverseNorth => fixed(res, traverse::cost(ctx, x, y, z, x, z - 1)),
            Moves::TraverseSouth => fixed(res, traverse::cost(ctx, x, y, z, x, z + 1)),
            Moves::TraverseEast => fixed(res, traverse::cost(ctx, x, y, z, x + 1, z)),
            Moves::TraverseWest => fixed(res, traverse::cost(ctx, x, y, z, x - 1, z)),
            Moves::AscendNorth => fixed(res, ascend::cost(ctx, x, y, z, x, z - 1)),
            Moves::AscendSouth => fixed(res, ascend::cost(ctx, x, y, z, x, z + 1)),
            Moves::AscendEast => fixed(res, ascend::cost(ctx, x, y, z, x + 1, z)),
            Moves::AscendWest => fixed(res, ascend::cost(ctx, x, y, z, x - 1, z)),
            Moves::DescendEast => descend::cost(ctx, x, y, z, x + 1, z, res),
            Moves::DescendWest => descend::cost(ctx, x, y, z, x - 1, z, res),
            Moves::DescendNorth => descend::cost(ctx, x, y, z, x, z - 1, res),
            Moves::DescendSouth => descend::cost(ctx, x, y, z, x, z + 1, res),
            Moves::DiagonalNortheast => diagonal::cost(ctx, x, y, z, x + 1, z - 1, res),
            Moves::DiagonalNorthwest => diagonal::cost(ctx, x, y, z, x - 1, z - 1, res),
            Moves::DiagonalSoutheast => diagonal::cost(ctx, x, y, z, x + 1, z + 1, res),
            Moves::DiagonalSouthwest => diagonal::cost(ctx, x, y, z, x - 1, z + 1, res),
            Moves::ParkourNorth => parkour::cost(ctx, x, y, z, Direction::North, res),
            Moves::ParkourSouth => parkour::cost(ctx, x, y, z, Direction::South, res),
            Moves::ParkourEast => parkour::cost(ctx, x, y, z, Direction::East, res),
            Moves::ParkourWest => parkour::cost(ctx, x, y, z, Direction::West, res),
        }
    }

    /// `apply0`: the movement itself, for building a path out of the positions the search found.
    pub fn apply0(self, ctx: &CalculationContext, src: BlockPos) -> Movement {
        let descend_or_fall = |dx: i32, dz: i32| {
            let mut res = MoveResult::new();
            descend::cost(ctx, src.x, src.y, src.z, src.x + dx, src.z + dz, &mut res);
            let dest = BlockPos::new(res.x, res.y, res.z);
            if res.y == src.y - 1 {
                Movement::descend(src, dest)
            } else {
                Movement::fall(src, dest)
            }
        };
        let diag = |d1: Direction, d2: Direction| {
            let mut res = MoveResult::new();
            let [ax, _, az] = d1.normal();
            let [bx, _, bz] = d2.normal();
            diagonal::cost(ctx, src.x, src.y, src.z, src.x + ax + bx, src.z + az + bz, &mut res);
            Movement::diagonal(src, d1, d2, res.y - src.y)
        };
        match self {
            Moves::Downward => Movement::downward(src, src.below()),
            Moves::Pillar => Movement::pillar(src, src.above()),
            Moves::TraverseNorth => Movement::traverse(src, src.north()),
            Moves::TraverseSouth => Movement::traverse(src, src.south()),
            Moves::TraverseEast => Movement::traverse(src, src.east()),
            Moves::TraverseWest => Movement::traverse(src, src.west()),
            Moves::AscendNorth => Movement::ascend(src, src.offset(0, 1, -1)),
            Moves::AscendSouth => Movement::ascend(src, src.offset(0, 1, 1)),
            Moves::AscendEast => Movement::ascend(src, src.offset(1, 1, 0)),
            Moves::AscendWest => Movement::ascend(src, src.offset(-1, 1, 0)),
            Moves::DescendEast => descend_or_fall(1, 0),
            Moves::DescendWest => descend_or_fall(-1, 0),
            Moves::DescendNorth => descend_or_fall(0, -1),
            Moves::DescendSouth => descend_or_fall(0, 1),
            Moves::DiagonalNortheast => diag(Direction::North, Direction::East),
            Moves::DiagonalNorthwest => diag(Direction::North, Direction::West),
            Moves::DiagonalSoutheast => diag(Direction::South, Direction::East),
            Moves::DiagonalSouthwest => diag(Direction::South, Direction::West),
            Moves::ParkourNorth => parkour::movement(ctx, src, Direction::North),
            Moves::ParkourSouth => parkour::movement(ctx, src, Direction::South),
            Moves::ParkourEast => parkour::movement(ctx, src, Direction::East),
            Moves::ParkourWest => parkour::movement(ctx, src, Direction::West),
        }
    }
}
