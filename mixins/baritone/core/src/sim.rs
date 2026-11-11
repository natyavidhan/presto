//! A crude stand-in for the game, to test the whole bot tick by tick: the player walks on level ground at
//! walking or sprinting speed, can't walk into blocks, breaks blocks as the game does (progress each tick
//! the attack key is held on the same block) and places dirt against the block under the crosshair.

use crate::baritone::Baritone;
use crate::math::{Aabb, BlockPos, Vec3};
use crate::pathing::tests::TestWorld;
use crate::player::{Controls, GameState, Inventory, PlayerState};
use crate::world::raytrace;
use crate::world::{BlockStateInterface, BlockTable, Dimension, HitResult, MapSource, WorldBorder, WorldView};
use std::sync::Arc;

pub struct Sim {
    pub table: Arc<BlockTable>,
    pub src: MapSource,
    pub game: GameState,
    pub b: Baritone,
    /// The block being broken and how far along.
    breaking: Option<(BlockPos, f32)>,
    pub ticks: u32,
    /// Ticks the attack key was held.
    pub attack_ticks: u32,
    /// Blocks broken, in order, with the tick each broke on.
    pub broken: Vec<(BlockPos, u32)>,
    /// Blocks placed.
    pub placed: Vec<BlockPos>,
}

impl Sim {
    pub fn new(world: TestWorld, feet: BlockPos) -> Self {
        let dir = std::env::temp_dir().join(format!("baritone-sim-{}-{:?}", std::process::id(), std::thread::current().id()));
        let mut b = Baritone::new(dir);
        // Nothing on disk, no chat spam from the tests.
        b.bot.settings_mut().chat_debug = false;
        let mut game = GameState::empty();
        let position = Vec3::new(feet.x as f64 + 0.5, feet.y as f64, feet.z as f64 + 0.5);
        game.player = Some(PlayerState {
            position,
            on_ground: true,
            eye_height: 1.62,
            health: 20.0,
            food: 20,
            entity_id: 1,
            name: "sim".into(),
            bounding_box: player_box(position),
            ..PlayerState::default()
        });
        game.inventory = Inventory::default();
        let mut sim = Self {
            table: world.table,
            src: world.src,
            game,
            b,
            breaking: None,
            ticks: 0,
            attack_ticks: 0,
            broken: Vec::new(),
            placed: Vec::new(),
        };
        sim.refresh_world();
        sim
    }

    fn view(&self) -> WorldView {
        WorldView {
            table: self.table.clone(),
            source: Arc::new(self.src.clone()),
            cache: None,
            dimension: Arc::new(Dimension::overworld()),
            border: WorldBorder::default(),
        }
    }

    fn refresh_world(&mut self) {
        let view = self.view();
        let player = self.game.player.as_ref().expect("sim player");
        let bsi = BlockStateInterface::new(&view, true);
        self.game.hit = raytrace::ray_trace_towards(&bsi, player.eyes(), player.rotation(), self.game.block_interaction_range);
        self.game.world = Some(view);
    }

    pub fn player(&self) -> &PlayerState {
        self.game.player.as_ref().expect("sim player")
    }

    pub fn feet(&self) -> BlockPos {
        self.player().feet_raw()
    }

    fn solid(&self, x: i32, y: i32, z: i32) -> bool {
        let s = self.src.blocks.get(&BlockPos::new(x, y, z)).copied().unwrap_or(self.src.default);
        !self.table.get(s).collision.is_empty()
    }

    /// One tick: Baritone decides, the game applies it.
    pub fn step(&mut self) -> Controls {
        // A real tick comes 50 ms after the last: give a search on its thread that long.
        if let Some(p) = self.b.bot.pathing.in_progress() {
            let until = std::time::Instant::now() + std::time::Duration::from_millis(50);
            while !p.shared.is_finished() && std::time::Instant::now() < until {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            // The result goes on the channel right after the search says it's finished.
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        self.refresh_world();
        let c = self.b.tick(&mut self.game);
        self.ticks += 1;
        let before = self.player().rotation();
        if let Some(look) = c.look {
            let p = self.game.player.as_mut().expect("sim player");
            p.yaw = look.rotation.yaw;
            p.pitch = look.rotation.pitch;
        }
        if let Some(slot) = c.select_slot {
            self.game.inventory.selected = slot;
        }
        // The game's crosshair after the turn.
        let view = self.view();
        let bsi = BlockStateInterface::new(&view, true);
        let player = self.player().clone();
        let hit = raytrace::ray_trace_towards(&bsi, player.eyes(), player.rotation(), self.game.block_interaction_range);
        self.attack(c.attack, hit);
        if c.use_item {
            self.use_item(hit);
        }
        if let Some(keys) = c.movement {
            self.walk(keys.impulse(), keys.sprint);
        } else {
            self.game.player.as_mut().expect("sim player").velocity = Vec3::new(0.0, 0.0, 0.0);
        }
        if let Some(look) = c.look
            && look.restore_after
        {
            let p = self.game.player.as_mut().expect("sim player");
            p.yaw = before.yaw;
            p.pitch = before.pitch;
        }
        c
    }

    fn attack(&mut self, held: bool, hit: HitResult) {
        if !held {
            self.breaking = None;
            return;
        }
        self.attack_ticks += 1;
        let HitResult::Block { pos, .. } = hit else {
            self.breaking = None;
            return;
        };
        let s = self.src.blocks.get(&pos).copied().unwrap_or(self.src.default);
        let info = self.table.get(s);
        if info.hardness < 0.0 || info.is_air() {
            return;
        }
        let per_tick = if info.hardness == 0.0 {
            1.0
        } else {
            1.0 / info.hardness / if info.requires_tool { 100.0 } else { 30.0 }
        };
        let progress = match self.breaking {
            Some((p, progress)) if p == pos => progress + per_tick,
            // MultiPlayerGameMode.startDestroyBlock: only an instant break breaks on the first tick.
            _ => {
                if per_tick >= 1.0 {
                    1.0
                } else {
                    0.0
                }
            }
        };
        if progress >= 1.0 {
            let air = self.table.air();
            self.src.set(pos, air);
            self.broken.push((pos, self.ticks));
            self.breaking = None;
        } else {
            self.breaking = Some((pos, progress));
        }
    }

    fn use_item(&mut self, hit: HitResult) {
        let HitResult::Block { pos, face, .. } = hit else { return };
        let held = self.game.inventory.held().clone();
        if held.is_empty() {
            return;
        }
        let Some(state) = self.table.default_state(&held.id) else { return };
        let at = pos.relative(face);
        if self.solid(at.x, at.y, at.z) || player_box(self.player().position).intersects(Aabb::block(at)) {
            return;
        }
        self.src.set(at, state);
        self.placed.push(at);
        let slot = self.game.inventory.selected;
        let stack = &mut self.game.inventory.items[slot];
        stack.count -= 1;
        if stack.count <= 0 {
            *stack = Default::default();
        }
    }

    /// Level walking: each axis moves unless the player would end up in a block.
    fn walk(&mut self, [strafe, forward]: [f32; 2], sprint: bool) {
        let p = self.player().clone();
        let speed = if sprint && forward > 0.0 { 0.2806 } else { 0.2158 };
        let len = (strafe * strafe + forward * forward).sqrt();
        if len < 1e-4 {
            self.game.player.as_mut().expect("sim player").velocity = Vec3::new(0.0, 0.0, 0.0);
            return;
        }
        let (s, f) = ((strafe / len) as f64, (forward / len) as f64);
        let yaw = (p.yaw as f64).to_radians();
        let dx = (s * yaw.cos() - f * yaw.sin()) * speed;
        let dz = (f * yaw.cos() + s * yaw.sin()) * speed;
        let mut pos = p.position;
        let fits = |sim: &Self, at: Vec3| {
            let bb = player_box(at);
            let (x0, x1) = (bb.min.x.floor() as i32, (bb.max.x - 1e-7).floor() as i32);
            let (z0, z1) = (bb.min.z.floor() as i32, (bb.max.z - 1e-7).floor() as i32);
            let y = at.y.floor() as i32;
            (x0..=x1).all(|x| (z0..=z1).all(|z| !sim.solid(x, y, z) && !sim.solid(x, y + 1, z)))
        };
        let mut moved = Vec3::new(0.0, 0.0, 0.0);
        let try_x = Vec3::new(pos.x + dx, pos.y, pos.z);
        if fits(self, try_x) {
            pos = try_x;
            moved.x = dx;
        }
        let try_z = Vec3::new(pos.x, pos.y, pos.z + dz);
        if fits(self, try_z) {
            pos = try_z;
            moved.z = dz;
        }
        let pl = self.game.player.as_mut().expect("sim player");
        pl.horizontal_collision = moved.x != dx || moved.z != dz;
        pl.position = pos;
        pl.velocity = moved;
        pl.bounding_box = player_box(pos);
        pl.sprinting = sprint;
    }

    /// Steps until `done` or `max` ticks; whether `done` happened.
    pub fn run_until(&mut self, max: u32, done: impl Fn(&Sim) -> bool) -> bool {
        for _ in 0..max {
            if done(self) {
                return true;
            }
            self.step();
        }
        done(self)
    }

}

fn player_box(p: Vec3) -> Aabb {
    Aabb::new(p.x - 0.3, p.y, p.z - 0.3, p.x + 0.3, p.y + 1.8, p.z + 0.3)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pathing::goals::GoalBlock;

    #[test]
    fn walks_to_a_goal() {
        let mut w = TestWorld::new();
        w.floor(-5, 30, -5, 5, 64);
        let mut sim = Sim::new(w, BlockPos::new(0, 65, 0));
        let goal = BlockPos::new(20, 65, 3);
        sim.b.procs.custom_goal.set_goal_and_path(Arc::new(GoalBlock::at(goal)));
        assert!(sim.run_until(300, |s| s.feet() == goal), "stuck at {} after {} ticks", sim.feet(), sim.ticks);
        // 20.2 blocks, sprinting most of the way, at 0.28 blocks a tick: about 75 ticks.
        assert!(sim.ticks < 90, "took {} ticks", sim.ticks);
        // And it stops there: the process lets go, the keys are released.
        let c = sim.step();
        assert!(c.movement.is_none_or(|m| !m.forward), "still walking: {c:?}");
        assert!(!sim.b.procs.custom_goal.is_active_now());
    }

    #[test]
    fn tunnels_without_idle_ticks() {
        let mut w = TestWorld::new();
        w.floor(-2, 12, -1, 1, 64);
        // A dirt wall three thick and two high across the only way.
        for x in 3..=5 {
            for z in -1..=1 {
                w.set(x, 65, z, "minecraft:dirt");
                w.set(x, 66, z, "minecraft:dirt");
            }
        }
        // Walls on both sides and a ceiling, so going around or over isn't possible.
        for x in -2..=12 {
            for y in 65..=67 {
                w.set(x, y, -2, "minecraft:bedrock");
                w.set(x, y, 2, "minecraft:bedrock");
            }
            for z in -1..=1 {
                w.set(x, 67, z, "minecraft:bedrock");
            }
        }
        let mut sim = Sim::new(w, BlockPos::new(0, 65, 0));
        let goal = BlockPos::new(9, 65, 0);
        sim.b.procs.custom_goal.set_goal_and_path(Arc::new(GoalBlock::at(goal)));
        assert!(sim.run_until(400, |s| s.feet() == goal), "stuck at {} after {} ticks", sim.feet(), sim.ticks);
        assert_eq!(sim.broken.len(), 6, "broke {:?}", sim.broken);
        // Dirt takes 16 ticks of holding attack by hand (start + 15). With no wait between blocks the
        // second block of each pair starts the tick after the first breaks.
        for pair in sim.broken.windows(2) {
            let gap = pair[1].1 - pair[0].1;
            assert!(gap >= 16, "{pair:?}");
        }
        let (first, last) = (sim.broken[0].1, sim.broken[5].1);
        // Six blocks in a row, two at a time with walking between pairs: no idle ticks on the blocks.
        assert!(last - first <= 5 * 16 + 3 * 6, "breaking took {} ticks: {:?}", last - first, sim.broken);
    }
}

