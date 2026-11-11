//! The world as Baritone reads it: block states from the client's loaded chunks (a snapshot the game hands
//! over, safe to read on the path-finding threads) and, beyond them, from Baritone's own cache of chunks it
//! has seen before (`cache`), as `BlockStateInterface` does.

pub mod block;
pub mod raytrace;

pub use block::{BlockId, BlockInfo, BlockTable, Fluid, HitResult, RawState, SlabType, StateId, flags};

use crate::cache::{CachedRegion, CachedWorld};
use crate::math::BlockPos;
use std::sync::Arc;

/// Blocks by position, readable from any thread: the loaded chunks of the client world as they were when
/// the snapshot was taken.
pub trait BlockSource: Send + Sync {
    /// The state at a position: air above and below the world and where no chunk is loaded.
    fn state(&self, x: i32, y: i32, z: i32) -> StateId;
    /// Whether the chunk holding column `x, z` is loaded.
    fn is_loaded(&self, x: i32, z: i32) -> bool;
}

/// A source with no chunks at all.
pub struct EmptySource(pub StateId);

impl BlockSource for EmptySource {
    fn state(&self, _: i32, _: i32, _: i32) -> StateId {
        self.0
    }
    fn is_loaded(&self, _: i32, _: i32) -> bool {
        false
    }
}

/// The dimension a world is in: its id and its height, as `DimensionType` gives them.
#[derive(Clone, Debug, PartialEq)]
pub struct Dimension {
    /// `minecraft:overworld`, `minecraft:the_nether`, `minecraft:the_end` or a data pack's.
    pub id: String,
    pub min_y: i32,
    /// `DimensionType.height()`.
    pub height: i32,
    /// `DimensionType.logicalHeight()`.
    pub logical_height: i32,
    pub has_ceiling: bool,
}

impl Dimension {
    pub fn overworld() -> Self {
        Self {
            id: "minecraft:overworld".into(),
            min_y: -64,
            height: 384,
            logical_height: 384,
            has_ceiling: false,
        }
    }
    pub fn nether() -> Self {
        Self {
            id: "minecraft:the_nether".into(),
            min_y: 0,
            height: 256,
            logical_height: 128,
            has_ceiling: true,
        }
    }
    pub fn end() -> Self {
        Self {
            id: "minecraft:the_end".into(),
            min_y: 0,
            height: 256,
            logical_height: 256,
            has_ceiling: false,
        }
    }
    pub fn is_nether(&self) -> bool {
        self.id == "minecraft:the_nether"
    }
    pub fn is_end(&self) -> bool {
        self.id == "minecraft:the_end"
    }
    pub fn is_overworld(&self) -> bool {
        self.id == "minecraft:overworld"
    }
    /// One above the highest y blocks can be at.
    pub fn max_y(&self) -> i32 {
        self.min_y + self.height
    }
    /// The folder name Baritone keeps the dimension's cache in (`DIM-1`, `DIM1`, or the id's path).
    pub fn cache_folder(&self) -> String {
        match self.id.as_str() {
            "minecraft:overworld" => "DIM0".into(),
            "minecraft:the_nether" => "DIM-1".into(),
            "minecraft:the_end" => "DIM1".into(),
            other => other.replace([':', '/'], "_"),
        }
    }
}

/// `BetterWorldBorder`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldBorder {
    pub min_x: f64,
    pub max_x: f64,
    pub min_z: f64,
    pub max_z: f64,
}

impl Default for WorldBorder {
    fn default() -> Self {
        Self {
            min_x: -29_999_984.0,
            max_x: 29_999_984.0,
            min_z: -29_999_984.0,
            max_z: 29_999_984.0,
        }
    }
}

impl WorldBorder {
    pub fn entirely_contains(&self, x: i32, z: i32) -> bool {
        (x + 1) as f64 > self.min_x
            && (x as f64) < self.max_x
            && (z + 1) as f64 > self.min_z
            && (z as f64) < self.max_z
    }
    /// One block in from the border on every side: a block against one outside can't be clicked.
    pub fn can_place_at(&self, x: i32, z: i32) -> bool {
        x as f64 > self.min_x
            && ((x + 1) as f64) < self.max_x
            && z as f64 > self.min_z
            && ((z + 1) as f64) < self.max_z
    }
}

/// Everything about the world a calculation reads, cheap to clone and safe to send to another thread.
#[derive(Clone)]
pub struct WorldView {
    pub table: Arc<BlockTable>,
    pub source: Arc<dyn BlockSource>,
    pub cache: Option<Arc<CachedWorld>>,
    pub dimension: Arc<Dimension>,
    pub border: WorldBorder,
}

/// `BlockStateInterface`: a block lookup that reads the loaded chunks first, then Baritone's cache.
pub struct BlockStateInterface {
    pub table: Arc<BlockTable>,
    source: Arc<dyn BlockSource>,
    cache: Option<Arc<CachedWorld>>,
    pub dimension: Arc<Dimension>,
    pub border: WorldBorder,
    use_the_real_world: bool,
    min_y: i32,
    height: i32,
    air: StateId,
    prev_cached: std::cell::Cell<Option<(i32, i32)>>,
    prev_region: std::cell::RefCell<Option<Arc<CachedRegion>>>,
}

impl BlockStateInterface {
    /// `use_the_real_world` is `!pathThroughCachedOnly`.
    pub fn new(view: &WorldView, use_the_real_world: bool) -> Self {
        Self {
            table: view.table.clone(),
            source: view.source.clone(),
            cache: view.cache.clone(),
            dimension: view.dimension.clone(),
            border: view.border,
            use_the_real_world,
            min_y: view.dimension.min_y,
            height: view.dimension.height,
            air: view.table.air(),
            prev_cached: std::cell::Cell::new(None),
            prev_region: std::cell::RefCell::new(None),
        }
    }

    pub fn view(&self) -> WorldView {
        WorldView {
            table: self.table.clone(),
            source: self.source.clone(),
            cache: self.cache.clone(),
            dimension: self.dimension.clone(),
            border: self.border,
        }
    }

    /// Whether the client has the chunk holding this column loaded (`worldContainsLoadedChunk`).
    #[inline]
    pub fn world_contains_loaded_chunk(&self, x: i32, z: i32) -> bool {
        self.source.is_loaded(x, z)
    }

    #[inline]
    pub fn get_pos(&self, pos: BlockPos) -> StateId {
        self.get(pos.x, pos.y, pos.z)
    }

    /// `get0`: the loaded world if the chunk is there, else the cached region, else air.
    #[inline]
    pub fn get(&self, x: i32, y: i32, z: i32) -> StateId {
        let adj = y - self.min_y;
        if adj < 0 || adj >= self.height {
            return self.air;
        }
        if self.use_the_real_world && self.source.is_loaded(x, z) {
            return self.source.state(x, y, z);
        }
        self.get_cached(x, y, z)
    }

    fn get_cached(&self, x: i32, y: i32, z: i32) -> StateId {
        let Some(cache) = &self.cache else {
            return self.air;
        };
        let key = (x >> 9, z >> 9);
        if self.prev_cached.get() != Some(key) {
            let region = cache.region(key.0, key.1);
            let Some(region) = region else {
                return self.air;
            };
            *self.prev_region.borrow_mut() = Some(region);
            self.prev_cached.set(Some(key));
        }
        let region = self.prev_region.borrow();
        match region.as_ref() {
            Some(r) => r
                .block(x & 511, y, z & 511, &self.dimension, &self.table)
                .unwrap_or(self.air),
            None => self.air,
        }
    }

    /// `isLoaded`: the loaded world or the cache has this column.
    pub fn is_loaded(&self, x: i32, z: i32) -> bool {
        if self.source.is_loaded(x, z) {
            return true;
        }
        let Some(cache) = &self.cache else {
            return false;
        };
        cache
            .region(x >> 9, z >> 9)
            .is_some_and(|r| r.is_cached(x & 511, z & 511))
    }

    #[inline]
    pub fn info(&self, x: i32, y: i32, z: i32) -> &BlockInfo {
        self.table.get(self.get(x, y, z))
    }

    #[inline]
    pub fn info_pos(&self, pos: BlockPos) -> &BlockInfo {
        self.info(pos.x, pos.y, pos.z)
    }

    pub fn air(&self) -> StateId {
        self.air
    }
}

/// A source over a fixed map of blocks, for tests and tools.
#[derive(Default)]
pub struct MapSource {
    pub blocks: rustc_hash::FxHashMap<BlockPos, StateId>,
    pub default: StateId,
    /// Loaded columns as chunk coordinates; empty means everything is loaded.
    pub loaded: rustc_hash::FxHashSet<(i32, i32)>,
}

impl MapSource {
    pub fn set(&mut self, pos: BlockPos, state: StateId) {
        self.blocks.insert(pos, state);
    }
}

impl BlockSource for MapSource {
    fn state(&self, x: i32, y: i32, z: i32) -> StateId {
        self.blocks
            .get(&BlockPos::new(x, y, z))
            .copied()
            .unwrap_or(self.default)
    }
    fn is_loaded(&self, x: i32, z: i32) -> bool {
        self.loaded.is_empty() || self.loaded.contains(&(x >> 4, z >> 4))
    }
}
