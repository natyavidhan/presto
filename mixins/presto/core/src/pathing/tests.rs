//! Path finding on small built worlds: the search finds the moves a player would make.

use super::calc::{AStarPathFinder, Favoring, ResultType, SearchLimits};
use super::context::CalculationContext;
use super::goals::{GoalBlock, GoalRef};
use super::movement::MovementKind;
use crate::math::BlockPos;
use crate::player::Inventory;
use crate::settings::Settings;
use crate::world::block::testing;
use crate::world::{BlockTable, Dimension, MapSource, StateId, WorldBorder, WorldView};
use std::sync::Arc;

pub struct TestWorld {
    pub table: Arc<BlockTable>,
    pub src: MapSource,
}

impl TestWorld {
    pub fn new() -> Self {
        let table = Arc::new(testing::vanilla());
        let src = MapSource {
            default: table.air(),
            ..Default::default()
        };
        Self { table, src }
    }

    pub fn state(&self, name: &str) -> StateId {
        self.table.default_state(name).unwrap()
    }

    pub fn set(&mut self, x: i32, y: i32, z: i32, name: &str) {
        let s = self.state(name);
        self.src.set(BlockPos::new(x, y, z), s);
    }

    /// A stone floor at `y`, over `x0..=x1` and `z0..=z1`.
    pub fn floor(&mut self, x0: i32, x1: i32, z0: i32, z1: i32, y: i32) {
        let s = self.state("minecraft:stone");
        for x in x0..=x1 {
            for z in z0..=z1 {
                self.src.set(BlockPos::new(x, y, z), s);
            }
        }
    }

    pub fn view(self) -> WorldView {
        WorldView {
            table: self.table,
            source: Arc::new(self.src),
            cache: None,
            dimension: Arc::new(Dimension::overworld()),
            border: WorldBorder::default(),
        }
    }
}

pub fn limits() -> SearchLimits {
    SearchLimits {
        primary_timeout_ms: 2000,
        failure_timeout_ms: 5000,
        slow_path: false,
        slow_path_delay_ms: 0,
        slow_path_timeout_ms: 0,
        max_chunk_border_fetch: 50,
        minimum_improvement_repropagation: true,
        map_default_size: 1024,
    }
}

fn search(view: &WorldView, settings: Settings, from: BlockPos, goal: GoalRef) -> super::calc::PathCalculationResult {
    let settings = Arc::new(settings);
    let ctx = CalculationContext::from_parts(view, &Inventory::default(), None, &settings, false, true);
    AStarPathFinder::new(from, from, goal, Favoring::default(), ctx).calculate(limits())
}

fn kinds(path: &super::path::Path) -> Vec<&'static str> {
    path.movements().iter().map(|m| m.kind.name()).collect()
}

#[test]
fn walks_across_flat_ground() {
    let mut w = TestWorld::new();
    w.floor(-5, 20, -5, 5, 63);
    let view = w.view();
    let r = search(&view, Settings::default(), BlockPos::new(0, 64, 0), Arc::new(GoalBlock::new(10, 64, 0)));
    assert_eq!(r.kind, ResultType::SuccessToGoal);
    let path = r.path.unwrap();
    assert_eq!(path.src(), BlockPos::new(0, 64, 0));
    assert_eq!(path.dest(), BlockPos::new(10, 64, 0));
    assert_eq!(path.len(), 11, "{:?}", path.positions());
    assert!(kinds(&path).iter().all(|k| *k == "MovementTraverse"), "{:?}", kinds(&path));
}

#[test]
fn takes_diagonals() {
    let mut w = TestWorld::new();
    w.floor(-5, 20, -5, 20, 63);
    let view = w.view();
    let r = search(&view, Settings::default(), BlockPos::new(0, 64, 0), Arc::new(GoalBlock::new(8, 64, 8)));
    assert_eq!(r.kind, ResultType::SuccessToGoal);
    let path = r.path.unwrap();
    assert_eq!(path.len(), 9, "{:?}", kinds(&path));
    assert!(kinds(&path).iter().all(|k| *k == "MovementDiagonal"));
}

#[test]
fn goes_around_a_wall_rather_than_through_it() {
    let mut w = TestWorld::new();
    w.floor(-5, 20, -10, 10, 63);
    for z in -3..=3 {
        for y in 64..=66 {
            w.set(5, y, z, "minecraft:obsidian");
        }
    }
    let view = w.view();
    let r = search(&view, Settings::default(), BlockPos::new(0, 64, 0), Arc::new(GoalBlock::new(10, 64, 0)));
    assert_eq!(r.kind, ResultType::SuccessToGoal);
    let path = r.path.unwrap();
    assert!(path.positions().iter().all(|p| !(p.x == 5 && (-3..=3).contains(&p.z))));
}

#[test]
fn climbs_and_descends_steps() {
    let mut w = TestWorld::new();
    w.floor(-2, 20, -2, 2, 63);
    // A two-step staircase up, then down the other side.
    for z in -2..=2 {
        w.set(4, 64, z, "minecraft:stone");
        w.set(5, 64, z, "minecraft:stone");
        w.set(5, 65, z, "minecraft:stone");
    }
    let view = w.view();
    let r = search(&view, Settings::default(), BlockPos::new(0, 64, 0), Arc::new(GoalBlock::new(9, 64, 0)));
    assert_eq!(r.kind, ResultType::SuccessToGoal);
    let path = r.path.unwrap();
    let k = kinds(&path);
    assert!(k.contains(&"MovementAscend"), "{k:?}");
    assert!(k.contains(&"MovementDescend") || k.contains(&"MovementFall"), "{k:?}");
    assert!(path.positions().contains(&BlockPos::new(5, 66, 0)) || path.positions().iter().any(|p| p.y == 66));
}

#[test]
fn falls_down_a_cliff() {
    let mut w = TestWorld::new();
    w.floor(-2, 2, -2, 2, 66);
    w.floor(3, 10, -2, 2, 63);
    let view = w.view();
    let r = search(&view, Settings::default(), BlockPos::new(0, 67, 0), Arc::new(GoalBlock::new(8, 64, 0)));
    assert_eq!(r.kind, ResultType::SuccessToGoal);
    let path = r.path.unwrap();
    let fall = path
        .movements()
        .iter()
        .find(|m| matches!(m.kind, MovementKind::Fall))
        .unwrap_or_else(|| panic!("{:?}", kinds(&path)));
    assert_eq!(fall.src.y - fall.dest.y, 3);
}

#[test]
fn refuses_falls_higher_than_max_fall_height() {
    // maxFallHeightNoWater is 3: a sheer seven-block drop with no water has no path down.
    let mut w = TestWorld::new();
    w.floor(-2, 2, -2, 2, 70);
    w.floor(3, 10, -2, 2, 63);
    let view = w.view();
    let settings = Settings {
        allow_place: false,
        allow_break: false,
        ..Settings::default()
    };
    let r = search(&view, settings, BlockPos::new(0, 71, 0), Arc::new(GoalBlock::new(8, 64, 0)));
    assert_ne!(r.kind, ResultType::SuccessToGoal);
    // With a pool at the bottom, the fall is fine.
    let mut w = TestWorld::new();
    w.floor(-2, 2, -2, 2, 70);
    w.floor(3, 10, -2, 2, 62);
    for z in -2..=2 {
        for x in 3..=10 {
            w.set(x, 63, z, "minecraft:water");
            w.set(x, 64, z, "minecraft:water");
        }
    }
    let view = w.view();
    let settings = Settings {
        allow_place: false,
        allow_break: false,
        ..Settings::default()
    };
    let r = search(&view, settings, BlockPos::new(0, 71, 0), Arc::new(GoalBlock::new(3, 64, 0)));
    assert_eq!(r.kind, ResultType::SuccessToGoal, "fall into water");
}

#[test]
fn jumps_a_gap() {
    let mut w = TestWorld::new();
    w.floor(-3, 0, -1, 1, 63);
    w.floor(3, 8, -1, 1, 63);
    // A bottomless pit at x=1 and x=2.
    let view = w.view();
    let parkour = Settings {
        allow_parkour: true,
        allow_place: false,
        allow_break: false,
        ..Settings::default()
    };
    let r = search(&view, parkour, BlockPos::new(0, 64, 0), Arc::new(GoalBlock::new(6, 64, 0)));
    assert_eq!(r.kind, ResultType::SuccessToGoal);
    let path = r.path.unwrap();
    assert!(kinds(&path).contains(&"MovementParkour"), "{:?}", kinds(&path));
    // allowParkour is off by default: no way across.
    let no_parkour = Settings {
        allow_place: false,
        allow_break: false,
        ..Settings::default()
    };
    let r = search(&view, no_parkour, BlockPos::new(0, 64, 0), Arc::new(GoalBlock::new(6, 64, 0)));
    assert_ne!(r.kind, ResultType::SuccessToGoal);
}

#[test]
fn breaks_through_when_there_is_no_way_around() {
    let mut w = TestWorld::new();
    w.floor(-1, 12, -1, 1, 63);
    for z in -1..=1 {
        w.set(5, 64, z, "minecraft:dirt");
        w.set(5, 65, z, "minecraft:dirt");
    }
    // A ceiling, so the dirt can't be climbed over.
    for x in -1..=12 {
        for z in -1..=1 {
            w.set(x, 66, z, "minecraft:bedrock");
        }
    }
    let view = w.view();
    let r = search(&view, Settings::default(), BlockPos::new(0, 64, 0), Arc::new(GoalBlock::new(10, 64, 0)));
    assert_eq!(r.kind, ResultType::SuccessToGoal);
    // With breaking off there is no path.
    let settings = Settings {
        allow_break: false,
        ..Settings::default()
    };
    let r = search(&view, settings, BlockPos::new(0, 64, 0), Arc::new(GoalBlock::new(10, 64, 0)));
    assert_ne!(r.kind, ResultType::SuccessToGoal);
}

/// A world held in one flat array, as fast to read as a real chunk snapshot.
pub struct GridSource {
    pub min: BlockPos,
    pub size: [i32; 3],
    pub blocks: Vec<StateId>,
    pub air: StateId,
}

impl GridSource {
    pub fn new(min: BlockPos, size: [i32; 3], air: StateId) -> Self {
        Self {
            min,
            size,
            blocks: vec![air; (size[0] * size[1] * size[2]) as usize],
            air,
        }
    }
    #[inline]
    fn index(&self, x: i32, y: i32, z: i32) -> Option<usize> {
        let (dx, dy, dz) = (x - self.min.x, y - self.min.y, z - self.min.z);
        if dx < 0 || dy < 0 || dz < 0 || dx >= self.size[0] || dy >= self.size[1] || dz >= self.size[2] {
            return None;
        }
        Some(((dy * self.size[2] + dz) * self.size[0] + dx) as usize)
    }
    pub fn set(&mut self, x: i32, y: i32, z: i32, s: StateId) {
        if let Some(i) = self.index(x, y, z) {
            self.blocks[i] = s;
        }
    }
}

impl crate::world::BlockSource for GridSource {
    #[inline]
    fn state(&self, x: i32, y: i32, z: i32) -> StateId {
        self.index(x, y, z).map_or(self.air, |i| self.blocks[i])
    }
    fn is_loaded(&self, x: i32, z: i32) -> bool {
        x >= self.min.x && z >= self.min.z && x < self.min.x + self.size[0] && z < self.min.z + self.size[2]
    }
}

/// Nodes per second over a large, uneven area. Run with `cargo test --release -- --ignored --nocapture`.
#[test]
#[ignore]
fn benchmark_long_search() {
    let table = Arc::new(testing::vanilla());
    let stone = table.default_state("minecraft:stone").unwrap();
    let mut g = GridSource::new(BlockPos::new(-320, -64, -320), [640, 384, 640], table.air());
    // Rolling terrain with scattered pillars.
    for x in -300..300 {
        for z in -300..300 {
            let h = 63 + (((x as f64 * 0.05).sin() + (z as f64 * 0.07).cos()) * 2.0) as i32;
            for y in 58..=h {
                g.set(x, y, z, stone);
            }
            if (x * 7 + z * 13).rem_euclid(29) == 0 {
                for y in h + 1..=h + 3 {
                    g.set(x, y, z, stone);
                }
            }
        }
    }
    let view = WorldView {
        table: table.clone(),
        source: Arc::new(g),
        cache: None,
        dimension: Arc::new(Dimension::overworld()),
        border: WorldBorder::default(),
    };
    let settings = Arc::new(Settings {
        allow_break: false,
        allow_place: false,
        ..Settings::default()
    });
    for (name, goal) in [
        ("GoalXZ 790 blocks", Arc::new(super::goals::GoalXZ::new(280, 270)) as GoalRef),
        ("GoalXZ 400 blocks", Arc::new(super::goals::GoalXZ::new(0, 0)) as GoalRef),
    ] {
        let ctx = CalculationContext::from_parts(&view, &Inventory::default(), None, &settings, false, true);
        let start = BlockPos::new(-280, 64 + 2, -280);
        // Stand on the terrain at the start.
        let mut start = start;
        while !crate::pathing::helper::can_walk_on(&ctx, start.x, start.y - 1, start.z) {
            start = start.below();
        }
        let finder = AStarPathFinder::new(start, start, goal, Favoring::default(), ctx);
        let shared = finder.shared();
        let t = std::time::Instant::now();
        let mut l = limits();
        l.primary_timeout_ms = 30_000;
        l.failure_timeout_ms = 30_000;
        let r = finder.calculate(l);
        let elapsed = t.elapsed();
        let nodes = shared.progress().nodes_considered;
        println!(
            "{name}: {:?}, {} nodes in {:?} = {:.0} nodes/s, path length {:?}",
            r.kind,
            nodes,
            elapsed,
            nodes as f64 / elapsed.as_secs_f64(),
            r.path.as_ref().map(|p| p.len())
        );
    }
}
