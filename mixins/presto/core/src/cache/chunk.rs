//! `CachedChunk` and `ChunkPacker`: a chunk boiled down to two bits a block (air, water, avoid, solid), its
//! surface blocks, and where its notable blocks (chests, portals, spawners, beds...) are.

use crate::math::BlockPos;
use crate::world::{BlockInfo, BlockSource, BlockTable, Dimension, Fluid, SlabType, StateId, flags};
use rustc_hash::FxHashMap;
use std::sync::Arc;

/// `PathingBlockType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PathingBlockType {
    Air,
    Water,
    Avoid,
    Solid,
}

impl PathingBlockType {
    /// The two bits as `PathingBlockType.getBits` lays them out.
    pub fn bits(self) -> (bool, bool) {
        match self {
            PathingBlockType::Air => (false, false),
            PathingBlockType::Water => (false, true),
            PathingBlockType::Avoid => (true, false),
            PathingBlockType::Solid => (true, true),
        }
    }
    pub fn from_bits(b1: bool, b2: bool) -> Self {
        match (b1, b2) {
            (true, true) => PathingBlockType::Solid,
            (true, false) => PathingBlockType::Avoid,
            (false, true) => PathingBlockType::Water,
            (false, false) => PathingBlockType::Air,
        }
    }
}

/// The blocks whose positions are remembered (`CachedChunk.BLOCKS_TO_KEEP_TRACK_OF`), by id path; every
/// shulker box and every bed too.
pub const BLOCKS_TO_KEEP_TRACK_OF: &[&str] = &[
    "ender_chest",
    "furnace",
    "chest",
    "trapped_chest",
    "end_portal",
    "end_portal_frame",
    "spawner",
    "barrier",
    "observer",
    "nether_portal",
    "hopper",
    "beacon",
    "brewing_stand",
    "creeper_head",
    "creeper_wall_head",
    "dragon_head",
    "dragon_wall_head",
    "player_head",
    "player_wall_head",
    "zombie_head",
    "zombie_wall_head",
    "skeleton_skull",
    "skeleton_wall_skull",
    "wither_skeleton_skull",
    "wither_skeleton_wall_skull",
    "enchanting_table",
    "anvil",
    "dragon_egg",
    "jukebox",
    "end_gateway",
    "cobweb",
    "nether_wart",
    "ladder",
    "vine",
];

/// Whether `ChunkPacker` remembers where this block is.
pub fn keeps_track_of(name: &str) -> bool {
    match name.strip_prefix("minecraft:") {
        Some(path) => {
            BLOCKS_TO_KEEP_TRACK_OF.contains(&path)
                || path.ends_with("shulker_box")
                || path.ends_with("_bed")
        }
        None => false,
    }
}

/// `BlockUtils.blockToString`: a vanilla block by its path, a mod's by its full id.
pub fn block_to_string(name: &str) -> &str {
    name.strip_prefix("minecraft:").unwrap_or(name)
}

/// `BlockUtils.stringToBlock`'s normalisation.
pub fn string_to_block(name: &str) -> String {
    crate::settings::normalize_id(name)
}

/// `CachedChunk.getPositionIndex`: two bits a block, x fastest, then z, then y.
#[inline]
pub const fn position_index(x: i32, y: i32, z: i32) -> usize {
    ((x << 1) | (z << 5) | (y << 9)) as usize
}

pub const fn size(height: i32) -> usize {
    2 * 16 * 16 * height as usize
}

pub const fn size_in_bytes(size: usize) -> usize {
    size / 8
}

/// A chunk as Presto remembers it.
#[derive(Clone, Debug)]
pub struct CachedChunk {
    pub x: i32,
    pub z: i32,
    /// The dimension's height when it was packed.
    pub height: i32,
    /// `BitSet.toByteArray()` order: bit `i` is bit `i % 8` of byte `i / 8`.
    data: Box<[u8]>,
    /// The top block of each column (`z << 4 | x`), by id.
    overview: Box<[Arc<str>]>,
    /// The resolved top blocks, for the table the chunk was resolved with.
    overview_states: Box<[StateId]>,
    height_map: Box<[i32]>,
    special: Option<FxHashMap<usize, Arc<str>>>,
    /// Where the tracked blocks are: x and z within the chunk, y absolute.
    special_locations: Vec<(Arc<str>, Vec<BlockPos>)>,
    /// When it was packed, in milliseconds since the epoch.
    pub cache_timestamp: i64,
}

impl CachedChunk {
    pub fn new(
        x: i32,
        z: i32,
        height: i32,
        data: Box<[u8]>,
        overview: Box<[Arc<str>]>,
        special_locations: Vec<(Arc<str>, Vec<BlockPos>)>,
        cache_timestamp: i64,
        table: &BlockTable,
        min_y: i32,
    ) -> Self {
        let mut chunk = CachedChunk {
            x,
            z,
            height,
            data,
            overview_states: overview
                .iter()
                .map(|n| table.default_state(n).unwrap_or(table.air()))
                .collect(),
            overview,
            height_map: vec![0; 256].into_boxed_slice(),
            special: None,
            special_locations,
            cache_timestamp,
        };
        if !chunk.special_locations.is_empty() {
            let mut special = FxHashMap::default();
            for (name, positions) in &chunk.special_locations {
                for p in positions {
                    let adj = p.y - min_y;
                    if (0..height).contains(&adj) {
                        special.insert(position_index(p.x, adj, p.z), name.clone());
                    }
                }
            }
            chunk.special = Some(special);
        }
        chunk.calculate_height_map();
        chunk
    }

    #[inline]
    fn bit(&self, i: usize) -> bool {
        self.data
            .get(i >> 3)
            .is_some_and(|b| b & (1 << (i & 7)) != 0)
    }

    fn get_type(&self, index: usize) -> PathingBlockType {
        PathingBlockType::from_bits(self.bit(index), self.bit(index + 1))
    }

    fn calculate_height_map(&mut self) {
        for z in 0..16 {
            for x in 0..16 {
                let index = (z << 4 | x) as usize;
                self.height_map[index] = 0;
                let mut y = self.height;
                while y >= 0 {
                    let i = position_index(x, y, z);
                    if self.bit(i) || self.bit(i + 1) {
                        self.height_map[index] = y;
                        break;
                    }
                    y -= 1;
                }
            }
        }
    }

    /// `CachedChunk.getBlock`, with `y` already relative to the dimension's bottom.
    pub fn get_block(&self, x: i32, y: i32, z: i32, dim: &Dimension, table: &BlockTable) -> StateId {
        let index = position_index(x, y, z);
        let ty = self.get_type(index);
        let internal = (z << 4 | x) as usize;
        if self.height_map[internal] == y && ty != PathingBlockType::Avoid {
            return self.overview_states[internal];
        }
        if let Some(special) = &self.special
            && let Some(name) = special.get(&index)
            && let Some(state) = table.default_state(name)
        {
            return state;
        }
        if ty == PathingBlockType::Solid {
            if y == dim.logical_height - 1 && dim.has_ceiling {
                // The Nether's roof is always bedrock.
                if let Some(b) = table.default_state("minecraft:bedrock") {
                    return b;
                }
            }
            // Solid blocks in the bottom five layers are often bedrock: read them as obsidian, so paths
            // prefer breaking what is known to be stone. (Java Baritone compares the relative y with the
            // absolute bottom here, which never matches below y=0 since 1.18; this uses the relative bottom.)
            if (dim.is_overworld() || dim.is_nether()) && y < 5 {
                return table.solid();
            }
        }
        pathing_type_to_block(ty, dim, table)
    }

    pub fn overview(&self) -> &[Arc<str>] {
        &self.overview
    }

    pub fn relative_blocks(&self) -> &[(Arc<str>, Vec<BlockPos>)] {
        &self.special_locations
    }

    /// The tracked blocks of one kind, in world coordinates (`getAbsoluteBlocks`).
    pub fn absolute_blocks(&self, name: &str) -> Vec<BlockPos> {
        self.special_locations
            .iter()
            .filter(|(n, _)| &**n == name)
            .flat_map(|(_, v)| v.iter())
            .map(|p| BlockPos::new(p.x + self.x * 16, p.y, p.z + self.z * 16))
            .collect()
    }

    pub fn bytes(&self) -> &[u8] {
        &self.data
    }
}

/// `ChunkPacker.pathingTypeToBlock`.
pub fn pathing_type_to_block(ty: PathingBlockType, dim: &Dimension, table: &BlockTable) -> StateId {
    match ty {
        PathingBlockType::Air => table.air(),
        PathingBlockType::Water => table.water(),
        PathingBlockType::Avoid => table.lava(),
        PathingBlockType::Solid => {
            let name = if dim.is_nether() {
                "minecraft:netherrack"
            } else if dim.is_end() {
                "minecraft:end_stone"
            } else {
                "minecraft:stone"
            };
            table.default_state(name).unwrap_or(table.solid())
        }
    }
}

/// `MovementHelper.possiblyFlowing`.
pub fn possibly_flowing(info: &BlockInfo) -> bool {
    matches!(info.fluid, Fluid::Water | Fluid::Lava) && info.fluid_amount != 8
}

/// `MovementHelper.avoidWalkingInto`.
pub fn avoid_walking_into(info: &BlockInfo, allow_walk_on_magma: bool) -> bool {
    info.has_fluid()
        || (info.is(flags::MAGMA) && !allow_walk_on_magma)
        || info.is(flags::CACTUS)
        || info.is(flags::SWEET_BERRY_BUSH)
        || info.is(flags::FIRE)
        || info.is(flags::END_PORTAL)
        || info.is(flags::COBWEB)
        || info.is(flags::BUBBLE_COLUMN)
}

/// `FlowingFluid.getFlow`'s horizontal part: whether water at `pos` would push sideways.
fn water_flows_sideways(src: &dyn BlockSource, table: &BlockTable, x: i32, y: i32, z: i32) -> bool {
    let own = table.get(src.state(x, y, z));
    let own_height = own.fluid_amount as f32 / 9.0;
    let affects = |i: &BlockInfo| i.fluid == Fluid::None || i.fluid == own.fluid;
    let (mut dx, mut dz) = (0.0f32, 0.0f32);
    for (sx, sz) in [(0, 1), (-1, 0), (0, -1), (1, 0)] {
        let n = table.get(src.state(x + sx, y, z + sz));
        if !affects(n) {
            continue;
        }
        let mut f = n.fluid_amount as f32 / 9.0;
        let mut f1 = 0.0f32;
        if f == 0.0 {
            if !n.is(flags::BLOCKS_MOTION) {
                let below = table.get(src.state(x + sx, y - 1, z + sz));
                if affects(below) {
                    f = below.fluid_amount as f32 / 9.0;
                    if f > 0.0 {
                        f1 = own_height - (f - 0.8888889);
                    }
                }
            }
        } else if f > 0.0 {
            f1 = own_height - f;
        }
        if f1 != 0.0 {
            dx += sx as f32 * f1;
            dz += sz as f32 * f1;
        }
    }
    dx != 0.0 || dz != 0.0
}

/// `ChunkPacker.getPathingBlockType`.
fn pathing_block_type(
    src: &dyn BlockSource,
    table: &BlockTable,
    info: &BlockInfo,
    cx: i32,
    cz: i32,
    x: i32,
    y: i32,
    z: i32,
    allow_walk_on_magma: bool,
) -> PathingBlockType {
    let wx = (cx << 4) + x;
    let wz = (cz << 4) + z;
    if info.is_water() {
        if possibly_flowing(info) {
            return PathingBlockType::Avoid;
        }
        let flowing_at = |dx: i32, dz: i32| possibly_flowing(table.get(src.state(wx + dx, y, wz + dz)));
        if (x != 15 && flowing_at(1, 0))
            || (x != 0 && flowing_at(-1, 0))
            || (z != 15 && flowing_at(0, 1))
            || (z != 0 && flowing_at(0, -1))
        {
            return PathingBlockType::Avoid;
        }
        if x == 0 || x == 15 || z == 0 || z == 15 {
            if water_flows_sideways(src, table, wx, y, wz) {
                return PathingBlockType::Water;
            }
            return PathingBlockType::Avoid;
        }
        return PathingBlockType::Water;
    }
    if avoid_walking_into(info, allow_walk_on_magma) || info.slab == SlabType::Bottom {
        return PathingBlockType::Avoid;
    }
    if info.is_air() || info.is(flags::PLANT) {
        return PathingBlockType::Air;
    }
    PathingBlockType::Solid
}

/// `ChunkPacker.pack`: packs chunk `cx, cz` from a source that has it loaded.
pub fn pack(
    src: &dyn BlockSource,
    table: &BlockTable,
    dim: &Dimension,
    cx: i32,
    cz: i32,
    allow_walk_on_magma: bool,
    now_ms: i64,
) -> CachedChunk {
    let height = dim.height;
    let mut data = vec![0u8; size_in_bytes(size(height))];
    let mut special: FxHashMap<Arc<str>, Vec<BlockPos>> = FxHashMap::default();
    let set = |data: &mut [u8], i: usize, v: bool| {
        if v {
            data[i >> 3] |= 1 << (i & 7);
        }
    };
    let air = table.air();
    for y in 0..height {
        let wy = y + dim.min_y;
        for z in 0..16 {
            for x in 0..16 {
                let state = src.state((cx << 4) + x, wy, (cz << 4) + z);
                if state == air {
                    continue;
                }
                let info = table.get(state);
                let index = position_index(x, y, z);
                let (b1, b2) =
                    pathing_block_type(src, table, info, cx, cz, x, wy, z, allow_walk_on_magma)
                        .bits();
                set(&mut data, index, b1);
                set(&mut data, index + 1, b2);
                if keeps_track_of(&info.name) {
                    special
                        .entry(info.name.clone())
                        .or_default()
                        .push(BlockPos::new(x, wy, z));
                }
            }
        }
    }
    let bit = |i: usize| data[i >> 3] & (1 << (i & 7)) != 0;
    let mut overview: Vec<Arc<str>> = Vec::with_capacity(256);
    let air_name: Arc<str> = Arc::from("minecraft:air");
    for z in 0..16 {
        for x in 0..16 {
            let mut top = air_name.clone();
            let mut y = height - 1;
            while y >= 0 {
                let index = position_index(x, y, z);
                if bit(index) || bit(index + 1) {
                    let s = src.state((cx << 4) + x, y + dim.min_y, (cz << 4) + z);
                    top = table.get(s).name.clone();
                    break;
                }
                y -= 1;
            }
            overview.push(top);
        }
    }
    let mut special_locations: Vec<(Arc<str>, Vec<BlockPos>)> = special.into_iter().collect();
    special_locations.sort_by(|a, b| a.0.cmp(&b.0));
    CachedChunk::new(
        cx,
        cz,
        height,
        data.into_boxed_slice(),
        overview.into_boxed_slice(),
        special_locations,
        now_ms,
        table,
        dim.min_y,
    )
}
