// Baritone's glue to Horizon: this module is added to the game as `crate::game::baritone`, a child of `game`, so it
// reads and drives the game's own fields. Each client tick it hands `baritone_core` a snapshot of the game and
// applies what Baritone answers: the turn, the movement keys, holding attack, clicking use, the hotbar slot, the
// chat. Path searches read the world on their own threads through `Snapshot`, which shares the client's chunks
// rather than copying them.
//
// (Module payloads can't carry inner attributes or `//!` comments, so the notes are plain comments.)

use super::{Game, Screen};
use baritone_core::behavior::look::LookCommand;
use baritone_core::math::{Aabb, BlockPos, Direction, Rotation, Vec3};
use baritone_core::player::{
    ChatLine, ClickEvent as BClick, Controls, EntityInfo, GameMode as BGameMode, GameState, Inventory,
    ItemStack as BStack, MovementKeys, PlayerState, Registries, ToolSpeeds,
};
use baritone_core::render::Primitive;
use baritone_core::utils::block_optional_meta::StateSet;
use baritone_core::world::block::{Fluid as BFluid, HitResult, RawState};
use baritone_core::world::{BlockSource, BlockTable, Dimension, SectionFill, StateId, WorldBorder, WorldView};
use horizon_client::chunk::Chunk;
use horizon_client::held::Source;
use horizon_client::text::{ClickEvent, Style, Text};
use horizon_world::registry::BlockStateId;
use baritone_core::FxHashMap;
use std::cell::RefCell;
use std::sync::Arc;
use winit::event::MouseButton;

thread_local! {
    // The window's Baritone. The client game runs on one thread; an integrated server's own game never ticks
    // as a client, so it never makes one.
    static GLUE: RefCell<Option<Glue>> = const { RefCell::new(None) };
}

struct Glue {
    b: baritone_core::Baritone,
    // The block table for the resources it was built from (the facts table's address and length).
    table: Option<((usize, usize), Arc<BlockTable>)>,
    // Each tool's mining speeds against every block, by item id.
    tools: FxHashMap<Arc<str>, Arc<ToolSpeeds>>,
    registries: Arc<Registries>,
    // The chunks as last seen, to pack the ones that unload since.
    snapshot: Option<Arc<Snapshot>>,
    // The view to put back once the tick is done (`LookMode::Server`), or the smoothed one (`smoothLook`).
    restore: Option<(f32, f32)>,
    smoothed: Option<Rotation>,
    // What Baritone holds this tick.
    movement: Option<MovementKeys>,
    attacking: bool,
    sprinting: bool,
}

impl Glue {
    fn new() -> Self {
        let dir = Game::game_dir().join("baritone");
        Self {
            b: baritone_core::Baritone::new(dir),
            table: None,
            tools: FxHashMap::default(),
            registries: Arc::default(),
            snapshot: None,
            restore: None,
            smoothed: None,
            movement: None,
            attacking: false,
            sprinting: false,
        }
    }
}

// The client's loaded chunks, shared with the world (an edit copies the section it changes), readable from the
// search threads.
pub(crate) struct Snapshot {
    origin: [i32; 2],
    width: i32,
    depth: i32,
    chunks: Vec<Option<Arc<Chunk>>>,
    min_y: i32,
    max_y: i32,
    air: StateId,
}

impl Snapshot {
    fn of(game: &Game<'_>) -> Snapshot {
        let world = &game.world;
        let (mut x0, mut z0, mut x1, mut z1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
        for &(x, z) in world.chunks.keys() {
            x0 = x0.min(x);
            z0 = z0.min(z);
            x1 = x1.max(x);
            z1 = z1.max(z);
        }
        let (width, depth) = if x0 > x1 { (0, 0) } else { (x1 - x0 + 1, z1 - z0 + 1) };
        let mut chunks = vec![None; (width.max(0) as usize) * (depth.max(0) as usize)];
        for (&(x, z), chunk) in &world.chunks {
            chunks[((z - z0) * width + (x - x0)) as usize] = Some(chunk.clone());
        }
        Snapshot {
            origin: [x0, z0],
            width,
            depth,
            chunks,
            min_y: world.shape.min_y,
            max_y: world.shape.min_y + world.shape.height,
            air: StateId(world.air.0),
        }
    }

    #[inline]
    fn chunk_at(&self, cx: i32, cz: i32) -> Option<&Chunk> {
        let (dx, dz) = (cx - self.origin[0], cz - self.origin[1]);
        if dx < 0 || dz < 0 || dx >= self.width || dz >= self.depth {
            return None;
        }
        self.chunks[(dz * self.width + dx) as usize].as_deref()
    }

    fn keys(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        self.chunks.iter().enumerate().filter_map(|(i, c)| {
            c.as_ref()?;
            let i = i as i32;
            Some((self.origin[0] + i % self.width, self.origin[1] + i / self.width))
        })
    }

    fn chunk_index(&self, cx: i32, cz: i32) -> Option<usize> {
        let (dx, dz) = (cx - self.origin[0], cz - self.origin[1]);
        if dx < 0 || dz < 0 || dx >= self.width || dz >= self.depth {
            return None;
        }
        Some((dz * self.width + dx) as usize)
    }
}

impl BlockSource for Snapshot {
    #[inline]
    fn state(&self, x: i32, y: i32, z: i32) -> StateId {
        if y < self.min_y || y >= self.max_y {
            return self.air;
        }
        match self.chunk_at(x >> 4, z >> 4) {
            Some(c) => StateId(c.get(x & 15, y, z & 15)),
            None => self.air,
        }
    }

    #[inline]
    fn is_loaded(&self, x: i32, z: i32) -> bool {
        self.chunk_at(x >> 4, z >> 4).is_some()
    }

    fn copy_section(&self, cx: i32, sy: i32, cz: i32, min_y: i32, out: &mut [StateId; 4096]) -> SectionFill {
        let Some(chunk) = self.chunk_at(cx, cz) else {
            return SectionFill::Unloaded;
        };
        let Some(s) = chunk.section_index(min_y + (sy << 4)) else {
            return SectionFill::Uniform(self.air);
        };
        let section = &chunk.sections[s];
        if let Some(v) = section.blocks.uniform() {
            return SectionFill::Uniform(StateId(v));
        }
        let mut buf = [0u32; 4096];
        section.unpack(&mut buf);
        for (o, v) in out.iter_mut().zip(buf.iter()) {
            *o = StateId(*v);
        }
        SectionFill::Filled
    }

    fn scan_section(
        &self,
        cx: i32,
        sy: i32,
        cz: i32,
        min_y: i32,
        wanted: &StateSet,
        out: &mut Vec<BlockPos>,
    ) -> bool {
        let Some(chunk) = self.chunk_at(cx, cz) else {
            return false;
        };
        let Some(s) = chunk.section_index(min_y + (sy << 4)) else {
            return true;
        };
        let section = &chunk.sections[s];
        // A section whose palette holds none of them is skipped without unpacking it.
        if !section.blocks.maybe_has(|v| wanted.contains(StateId(v))) {
            return true;
        }
        let mut buf = [0u32; 4096];
        section.unpack(&mut buf);
        let (bx, by, bz) = (cx << 4, min_y + (sy << 4), cz << 4);
        for (i, v) in buf.iter().enumerate() {
            if wanted.contains(StateId(*v)) {
                let i = i as i32;
                out.push(BlockPos::new(bx + (i & 15), by + (i >> 8), bz + ((i >> 4) & 15)));
            }
        }
        true
    }
}

fn aabb(b: &horizon_core::geometry::Aabb) -> Aabb {
    let (lo, hi) = (b.min_position(), b.max_position());
    Aabb::new(lo.x, lo.y, lo.z, hi.x, hi.y, hi.z)
}

fn boxes(raw: &[[f64; 6]]) -> Vec<Aabb> {
    raw.iter().map(|b| Aabb::new(b[0], b[1], b[2], b[3], b[4], b[5])).collect()
}

fn face(normal: [i32; 3]) -> Direction {
    match normal {
        [0, -1, 0] => Direction::Down,
        [0, 1, 0] => Direction::Up,
        [0, 0, -1] => Direction::North,
        [0, 0, 1] => Direction::South,
        [-1, 0, 0] => Direction::West,
        _ => Direction::East,
    }
}

impl Game<'_> {
    // Every block state the game has, described for Baritone (`BlockTable`).
    fn baritone_table(&self) -> BlockTable {
        let facts = self.res.facts();
        let registry = &self.pack.assets.registry;
        let shapes = self.res.collision().cached_shapes();
        let mut raw = Vec::with_capacity(facts.states.len());
        let mut defaults: FxHashMap<String, StateId> = FxHashMap::default();
        for (i, f) in facts.states.iter().enumerate() {
            let id = BlockStateId(i as u32);
            let properties = registry
                .state(id)
                .map(|s| s.properties.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect())
                .unwrap_or_default();
            let outline = boxes(&f.outline);
            // The collision shape with no context; a shape that depends on its surroundings (scaffolding,
            // powder snow) falls back to the outline.
            let collision = match shapes.cached_shape(id) {
                Ok(Some(shape)) => shape.boxes().iter().map(aabb).collect(),
                _ => outline.clone(),
            };
            let fluid = match f.fluid {
                horizon_client::blocks::Fluid::None => BFluid::None,
                horizon_client::blocks::Fluid::Water => BFluid::Water,
                horizon_client::blocks::Fluid::Lava => BFluid::Lava,
                #[allow(unreachable_patterns)]
                _ => BFluid::Other,
            };
            if !defaults.contains_key(&*f.name)
                && let Some(d) = registry.default_state(&f.name)
            {
                defaults.insert(f.name.to_string(), StateId(d.0));
            }
            raw.push(RawState {
                name: f.name.to_string(),
                classes: f.classes.iter().map(|c| c.to_string()).collect(),
                properties,
                hardness: f.hardness,
                requires_tool: f.requires_tool,
                replaceable: f.replaceable,
                fluid,
                fluid_amount: f.fluid_level,
                fluid_source: fluid != BFluid::None && f.fluid_level == 8 && !f.fluid_falling,
                collision,
                outline,
                pathfindable_land: None,
                full_collision: Some(f.full_collision),
                legacy_solid: Some(f.legacy_solid),
                item: f.item.to_string(),
            });
        }
        let mut table = BlockTable::build(&raw, &defaults);
        // What breaking each block drops (`BlockOptionalMeta.drops`): its loot table thrown with an empty hand a
        // few times, so chancy drops (gravel's flint) are seen, and the block itself, for silk touch.
        let items = self.res.items();
        for i in 0..table.blocks().len() {
            let def = table.block(baritone_core::world::BlockId(i as u32));
            let state = BlockStateId(def.default_state.0);
            let mut drops: Vec<Arc<str>> = Vec::new();
            for seed in 1..=4 {
                let mut random = horizon_core::random::LegacyRandom::new(seed);
                for stack in items.block_drops(registry, state, None, &mut random) {
                    if !drops.iter().any(|d| *d == stack.item) {
                        drops.push(stack.item.clone());
                    }
                }
            }
            let own = table.get(def.default_state).item.clone();
            if &*own != "minecraft:air" && !drops.contains(&own) {
                drops.push(own);
            }
            table.set_drops(baritone_core::world::BlockId(i as u32), drops);
        }
        table
    }

    // A tool's speeds and harvest rules against every block, as the game's `tool` components answer them.
    fn baritone_tool_speeds(&self, table: &BlockTable, stack: &horizon_client::items::ItemStack) -> ToolSpeeds {
        let items = self.res.items();
        let mut speed = Vec::with_capacity(table.blocks().len());
        let mut correct = Vec::with_capacity(table.blocks().len());
        for b in table.blocks() {
            speed.push(items.mining_speed(Some(stack), &b.name));
            correct.push(items.correct_for_drops(Some(stack), &b.name));
        }
        ToolSpeeds {
            speed,
            correct_for_drops: correct,
        }
    }

    fn baritone_stack(
        &self,
        table: &BlockTable,
        tools: &mut FxHashMap<Arc<str>, Arc<ToolSpeeds>>,
        s: &Option<horizon_client::items::ItemStack>,
    ) -> BStack {
        let Some(s) = s else { return BStack::default() };
        let items = self.res.items();
        let info = items.get(&s.item);
        let is_tool = info.is_some_and(|i| !i.tool.is_empty());
        let speeds = is_tool.then(|| {
            tools
                .entry(s.item.clone())
                .or_insert_with(|| Arc::new(self.baritone_tool_speeds(table, s)))
                .clone()
        });
        let path = s.item.strip_prefix("minecraft:").unwrap_or(&s.item);
        let material_cost = ["wooden_", "stone_", "iron_", "golden_", "diamond_", "netherite_"]
            .iter()
            .position(|m| path.starts_with(m))
            .map_or(-1, |i| i as i32);
        let efficiency = s.enchantment_level("minecraft:efficiency");
        let depth_strider = s.enchantment_level("minecraft:depth_strider");
        BStack {
            id: s.item.clone(),
            count: s.count as i32,
            damage: s.damage as i32,
            max_damage: info.map_or(0, |i| i.max_damage as i32),
            is_sword: items.item_in_tag("minecraft:swords", &s.item),
            is_tool,
            material_cost,
            silk_touch: s.enchantment_level("minecraft:silk_touch") > 0,
            mining_efficiency: if efficiency > 0 { (efficiency * efficiency + 1) as f32 } else { 0.0 },
            frost_walker: s.enchantment_level("minecraft:frost_walker") as i32,
            water_movement_efficiency: depth_strider as f32 / 3.0,
            places_block: info
                .and_then(|i| i.block)
                .map(|b| table.get(StateId(b.0)).name.clone()),
            speeds,
            edible: info.is_some_and(|i| i.food.is_some()),
        }
    }

    // `<game>/baritone/<world or server>`: where this world's cache and waypoints go.
    fn baritone_world_dir(&self) -> std::path::PathBuf {
        let name = match (&self.integrated, self.server_address()) {
            (Some(server), _) => format!("world_{}", server.world_name),
            (None, Some(address)) => address.to_string(),
            (None, None) => format!("world_{}", self.level.name),
        };
        let safe: String = name
            .chars()
            .map(|c| if c.is_alphanumeric() || "-_. ".contains(c) { c } else { '_' })
            .collect();
        Game::game_dir().join("baritone").join(safe)
    }

    // The game as Baritone sees it this tick.
    fn baritone_state(&self, g: &mut Glue) -> GameState {
        let mut state = GameState::empty();
        state.now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        if self.title.is_some() || self.player.is_none() {
            g.snapshot = None;
            return state;
        }
        let key = (self.res.facts().states.as_ptr() as usize, self.res.facts().states.len());
        let table = match &g.table {
            Some((k, t)) if *k == key => t.clone(),
            _ => {
                let t = Arc::new(self.baritone_table());
                g.tools.clear();
                g.table = Some((key, t.clone()));
                g.registries = Arc::new(Registries {
                    items: self.res.items().items.keys().cloned().collect(),
                    entity_types: Vec::new(),
                });
                t
            }
        };
        let snapshot = Arc::new(Snapshot::of(self));
        let mut dimension = match self.dim.id() {
            "minecraft:the_nether" => Dimension::nether(),
            "minecraft:the_end" => Dimension::end(),
            _ => Dimension::overworld(),
        };
        let shape = &self.world.shape;
        dimension.min_y = shape.min_y;
        dimension.height = shape.height;
        dimension.logical_height = shape.logical_height;
        dimension.has_ceiling = shape.has_ceiling;
        // Chunks that unloaded since last tick go to the cache from last tick's snapshot, which still holds them;
        // new ones from this tick's.
        let old = g.snapshot.replace(snapshot.clone());
        if let Some(old) = &old {
            for (cx, cz) in old.keys() {
                if snapshot.chunk_index(cx, cz).and_then(|i| snapshot.chunks[i].as_ref()).is_none() {
                    g.b.on_chunk(cx, cz, old.clone());
                }
            }
        }
        for (cx, cz) in snapshot.keys() {
            if old.as_ref().is_none_or(|o| o.chunk_index(cx, cz).and_then(|i| o.chunks[i].as_ref()).is_none()) {
                g.b.on_chunk(cx, cz, snapshot.clone());
            }
        }
        state.world = Some(WorldView {
            table: table.clone(),
            source: snapshot,
            cache: None,
            dimension: Arc::new(dimension),
            border: WorldBorder::default(),
        });
        state.world_dir = Some(self.baritone_world_dir());
        let p = self.player.as_ref().expect("checked above");
        let effect = |id: &str| {
            self.potions
                .effects
                .list
                .iter()
                .find(|e| &*e.id == id)
                .map(|e| e.amplifier as i32)
        };
        let feet = BlockPos::of(p.position.x, p.position.y, p.position.z);
        let facts = self.res.facts();
        let lava = |pos: BlockPos| {
            facts.get(self.world.block(pos.x, pos.y, pos.z)).fluid == horizon_client::blocks::Fluid::Lava
        };
        state.player = Some(PlayerState {
            position: Vec3::new(p.position.x, p.position.y, p.position.z),
            velocity: Vec3::new(p.velocity.x, p.velocity.y, p.velocity.z),
            on_ground: p.ground.on_ground,
            horizontal_collision: p.collision.horizontal,
            yaw: self.yaw,
            pitch: self.pitch,
            eye_height: f64::from(p.dimensions.eye_height),
            crouching: p.pose == horizon_client::pose::Pose::Crouching,
            sprinting: p.sprinting,
            in_water: p.fluids.was_touching_water,
            in_lava: lava(feet) || lava(feet.above()),
            in_wall: false,
            flying: p.flying,
            may_fly: p.may_fly,
            fall_flying: p.fall_flying,
            health: p.health.health(),
            food: p.health.food.food as i32,
            game_mode: match self.mode {
                horizon_client::command::args::GameMode::Survival => BGameMode::Survival,
                horizon_client::command::args::GameMode::Creative => BGameMode::Creative,
                horizon_client::command::args::GameMode::Adventure => BGameMode::Adventure,
                horizon_client::command::args::GameMode::Spectator => BGameMode::Spectator,
            },
            bounding_box: aabb(&p.bounds()),
            // The local player isn't among the mobs Baritone sees, so no id is needed to tell it apart.
            entity_id: -1,
            name: String::new(),
            haste: effect("minecraft:haste"),
            mining_fatigue: effect("minecraft:mining_fatigue"),
            hands_busy: false,
            passenger: p.is_passenger,
        });
        let mut inv = Inventory {
            items: Vec::with_capacity(36),
            armor: Vec::with_capacity(4),
            offhand: BStack::default(),
            selected: self.selected,
            carried: BStack::default(),
        };
        let mut tools = std::mem::take(&mut g.tools);
        for s in self.inventory.iter().take(36) {
            inv.items.push(self.baritone_stack(&table, &mut tools, s));
        }
        // Horizon keeps the armor head first; Baritone feet first.
        for s in self.armor.iter().rev() {
            inv.armor.push(self.baritone_stack(&table, &mut tools, s));
        }
        inv.offhand = self.baritone_stack(&table, &mut tools, &self.offhand);
        inv.carried = self.baritone_stack(&table, &mut tools, &self.carried);
        g.tools = tools;
        state.inventory = inv;
        state.hit = match self.pick() {
            Some(hit) => HitResult::Block {
                pos: BlockPos::new(hit.pos[0], hit.pos[1], hit.pos[2]),
                face: face(hit.face),
                location: Vec3::new(hit.point[0], hit.point[1], hit.point[2]),
                inside: false,
            },
            None => HitResult::Miss,
        };
        for m in self.mobs.iter().filter(|m| !m.removed) {
            let is_player = m.kind.id == "minecraft:player";
            state.entities.push(EntityInfo {
                id: m.id as i32,
                kind: Arc::from(m.kind.id.as_str()),
                position: Vec3::new(m.position.x, m.position.y, m.position.z),
                bounding_box: aabb(&m.bounds()),
                alive: m.alive(),
                name: self.remote_player_name(m.id).map(Arc::from),
                uuid: self.remote_player_uuid(m.id).unwrap_or(0),
                item: None,
                is_player,
                hostile: m.kind.category == "monster",
                is_mob: !is_player,
                angry: false,
                falling_block: false,
                blocks_building: true,
                living: true,
            });
        }
        for item in self.dropped.iter() {
            let pos = item.position;
            state.entities.push(EntityInfo {
                id: item.id as i32,
                kind: Arc::from("minecraft:item"),
                position: Vec3::new(pos.x, pos.y, pos.z),
                bounding_box: Aabb::new(pos.x - 0.125, pos.y, pos.z - 0.125, pos.x + 0.125, pos.y + 0.25, pos.z + 0.125),
                alive: true,
                name: None,
                uuid: 0,
                item: Some(item.stack.item.clone()),
                is_player: false,
                hostile: false,
                is_mob: false,
                angry: false,
                falling_block: false,
                blocks_building: false,
                living: false,
            });
        }
        state.screen_open = self.screen != Screen::Playing && self.screen != Screen::Chat;
        state.chat_open = self.screen == Screen::Chat;
        state.container_open = self.container.is_some() || self.station.is_some();
        state.block_interaction_range = self.block_reach;
        state.singleplayer = self.integrated.is_some();
        state.mouse_sensitivity = f64::from(self.settings.sensitivity);
        state.registries = g.registries.clone();
        state
    }

    // The client tick begins (`ClientTickEvent.Pre`): Baritone decides, and its decisions go into the game before
    // the tick picks the crosshair's target, handles clicks and moves the player.
    pub(in crate::game) fn baritone_tick_pre(&mut self) {
        let controls = GLUE.with_borrow_mut(|g| {
            let g = g.get_or_insert_with(Glue::new);
            let mut state = self.baritone_state(g);
            g.b.tick(&mut state)
        });
        self.baritone_apply(controls);
    }

    fn baritone_apply(&mut self, c: Controls) {
        for line in &c.chat {
            self.system_message(baritone_text(line));
        }
        for (title, message) in &c.toasts {
            self.system_message(Text::colored(format!("{title}: {message}"), 0xFFFF55));
        }
        for n in &c.notifications {
            self.system_message(Text::colored(format!("[Baritone] {}", n.message), if n.error { 0xFF5555 } else { 0x55FF55 }));
        }
        let in_world = self.title.is_none() && self.player.is_some();
        let (was_attacking, was_sprinting) = GLUE.with_borrow_mut(|g| {
            let Some(g) = g.as_mut() else { return (false, false) };
            g.movement = if in_world { c.movement } else { None };
            g.restore = None;
            g.smoothed = None;
            if let Some(LookCommand { restore_after, smoothed, .. }) = c.look {
                if restore_after {
                    g.restore = Some((self.yaw, self.pitch));
                }
                g.smoothed = smoothed;
            }
            let was = (g.attacking, g.sprinting);
            g.attacking = c.attack && in_world;
            g.sprinting = c.movement.is_some_and(|m| m.sprint) && in_world;
            was
        });
        if !in_world {
            if was_attacking {
                let _ = self.mouse_mapping(MouseButton::Left, false, Source::Agent);
            }
            if was_sprinting {
                self.baritone_sprint(false);
            }
            return;
        }
        if let Some(look) = c.look {
            self.yaw = look.rotation.yaw;
            self.pitch = look.rotation.pitch.clamp(-90.0, 90.0);
        }
        if let Some(slot) = c.select_slot.filter(|s| *s < 9) {
            self.selected = slot;
        }
        if c.attack != was_attacking {
            let _ = self.mouse_mapping(MouseButton::Left, c.attack, Source::Agent);
        }
        if c.use_item {
            let _ = self.mouse_mapping(MouseButton::Right, true, Source::Agent);
            let _ = self.mouse_mapping(MouseButton::Right, false, Source::Agent);
        }
        let sprint = c.movement.is_some_and(|m| m.sprint);
        if sprint != was_sprinting {
            self.baritone_sprint(sprint);
        }
        if c.stop_flying
            && let Some(p) = self.player.as_mut()
        {
            p.flying = false;
        }
    }

    fn baritone_sprint(&mut self, on: bool) {
        if let Some(key) = horizon_client::actions::Action::Sprint.key() {
            if on {
                self.keys.press(key, Source::Agent);
            } else {
                self.keys.release(&key, Source::Agent);
            }
        }
    }

    // The client tick ends: a turn made only for the tick (`freeLook`) is taken back from the view.
    pub(in crate::game) fn baritone_tick_post(&mut self) {
        let (restore, smoothed) = GLUE.with_borrow_mut(|g| {
            g.as_mut()
                .map(|g| (g.restore.take(), g.smoothed.take()))
                .unwrap_or_default()
        });
        if let Some((yaw, pitch)) = restore {
            self.yaw = yaw;
            self.pitch = pitch;
        } else if let Some(r) = smoothed {
            self.yaw = r.yaw;
            self.pitch = r.pitch;
        }
    }

    // Draws the path and the goal (`PathRenderer`).
    pub(in crate::game) fn baritone_render(&self, primitives: &mut horizon_client::world_primitives::WorldPrimitives) {
        let player = self
            .player
            .as_ref()
            .map_or(Vec3::new(0.0, 0.0, 0.0), |p| Vec3::new(p.position.x, p.position.y, p.position.z));
        let list = GLUE.with_borrow(|g| {
            g.as_ref()
                .map(|g| baritone_core::render::primitives(&g.b, player))
                .unwrap_or_default()
        });
        for p in list {
            match p {
                Primitive::Line {
                    from,
                    to,
                    color,
                    width,
                    through_walls,
                } => {
                    let l = primitives.line(from, to, color, width);
                    if through_walls {
                        l.through_walls();
                    }
                }
                Primitive::Box {
                    min,
                    max,
                    color,
                    width,
                    through_walls,
                } => {
                    let b = primitives.box_outline(min, max, color, width);
                    if through_walls {
                        b.through_walls();
                    }
                }
            }
        }
    }
}

// Baritone's chat line as the game's text.
fn baritone_text(line: &ChatLine) -> Text {
    let mut text = Text::literal("");
    for s in &line.spans {
        let style = Style {
            color: s.color,
            bold: s.bold.then_some(true),
            italic: s.italic.then_some(true),
            underlined: s.underlined.then_some(true),
            strikethrough: s.strikethrough.then_some(true),
            click: s.click.as_ref().map(|c| match c {
                BClick::RunCommand(cmd) => ClickEvent::RunCommand(cmd.clone()),
                BClick::SuggestCommand(cmd) => ClickEvent::SuggestCommand(cmd.clone()),
            }),
            hover: s.hover.as_ref().map(|h| Box::new(Text::literal(h.clone()))),
            ..Style::default()
        };
        text.push(Text::styled(s.text.clone(), style));
    }
    text
}

// Whether Baritone holds the attack key: the game's 5-tick wait after each broken block is then left to Baritone's
// own `blockBreakSpeed` (Java Baritone zeroes `destroyDelay` too).
pub(in crate::game) fn attacking() -> bool {
    GLUE.with_borrow(|g| g.as_ref().is_some_and(|g| g.attacking))
}

// The movement Baritone holds this tick, if it controls the player (`PlayerMovementInput`).
pub(in crate::game) fn movement(forward: &mut f32, strafe: &mut f32, jump: &mut bool, sneak: &mut bool) {
    GLUE.with_borrow(|g| {
        if let Some(m) = g.as_ref().and_then(|g| g.movement) {
            let [left, fwd] = m.impulse();
            *forward = fwd;
            *strafe = left;
            *jump = m.jump;
            *sneak = m.sneak;
        }
    });
}

// A line the player sends from the chat box: true when it is Baritone's (`#goto ...`).
pub(in crate::game) fn chat(message: &str) -> bool {
    GLUE.with_borrow_mut(|g| g.get_or_insert_with(Glue::new).b.on_send_chat(message))
}

// `/baritone ...`, and the commands Baritone's clickable chat runs.
pub(in crate::game) fn command(command: &str) -> bool {
    GLUE.with_borrow_mut(|g| {
        let b = &mut g.get_or_insert_with(Glue::new).b;
        if b.on_chat_click(command) {
            return true;
        }
        match command.split_once(' ') {
            Some(("baritone", rest)) => {
                b.on_command(rest);
                true
            }
            None if command == "baritone" => {
                b.on_command("");
                true
            }
            _ => false,
        }
    })
}
