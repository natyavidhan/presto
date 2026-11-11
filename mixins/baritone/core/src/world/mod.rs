//! The world as Baritone reads it: block states from the client's loaded chunks (a snapshot the game hands
//! over, safe to read on the path-finding threads) and, beyond them, from Baritone's own cache of chunks it
//! has seen before (`cache`), as `BlockStateInterface` does.

pub mod block;
pub mod raytrace;

pub use block::{BlockId, BlockInfo, BlockTable, Fluid, HitResult, RawState, SlabType, StateId, flags};

use crate::cache::{CachedRegion, CachedWorld};
use crate::math::BlockPos;
use std::sync::Arc;

/// How [`BlockSource::copy_section`] filled a section.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectionFill {
    /// The chunk isn't loaded: nothing was written.
    Unloaded,
    /// Every block is this state: nothing was written.
    Uniform(StateId),
    /// The section's 4096 states were written, `y << 8 | z << 4 | x`.
    Filled,
}

/// Blocks by position, readable from any thread: the loaded chunks of the client world as they were when
/// the snapshot was taken.
pub trait BlockSource: Send + Sync {
    /// The state at a position: air above and below the world and where no chunk is loaded.
    fn state(&self, x: i32, y: i32, z: i32) -> StateId;
    /// Whether the chunk holding column `x, z` is loaded.
    fn is_loaded(&self, x: i32, z: i32) -> bool;
    /// Copies the 16-block section `sy` (counted from the dimension's bottom, `min_y`) of chunk `cx, cz` into
    /// `out`. Path searches read every block through these copies, so a source that can unpack a section in
    /// one go (straight from its palette) should override this.
    fn copy_section(&self, cx: i32, sy: i32, cz: i32, min_y: i32, out: &mut [StateId; 4096]) -> SectionFill {
        if !self.is_loaded(cx << 4, cz << 4) {
            return SectionFill::Unloaded;
        }
        let (bx, by, bz) = (cx << 4, min_y + (sy << 4), cz << 4);
        let first = self.state(bx, by, bz);
        let mut uniform = true;
        for y in 0..16 {
            for z in 0..16 {
                for x in 0..16 {
                    let s = self.state(bx + x, by + y, bz + z);
                    uniform &= s == first;
                    out[(y << 8 | z << 4 | x) as usize] = s;
                }
            }
        }
        if uniform {
            SectionFill::Uniform(first)
        } else {
            SectionFill::Filled
        }
    }

    /// Adds to `out` every block of section `sy` of chunk `cx, cz` whose state is in `wanted`, in
    /// `y, z, x` order. Returns false when the chunk isn't loaded. A source with palettes should override
    /// this to skip sections whose palette holds none of `wanted` without unpacking them.
    fn scan_section(
        &self,
        cx: i32,
        sy: i32,
        cz: i32,
        min_y: i32,
        wanted: &crate::utils::block_optional_meta::StateSet,
        out: &mut Vec<BlockPos>,
    ) -> bool {
        let mut buf = [StateId(0); 4096];
        let (bx, by, bz) = (cx << 4, min_y + (sy << 4), cz << 4);
        match self.copy_section(cx, sy, cz, min_y, &mut buf) {
            SectionFill::Unloaded => false,
            SectionFill::Uniform(s) => {
                if wanted.contains(s) {
                    for y in 0..16 {
                        for z in 0..16 {
                            for x in 0..16 {
                                out.push(BlockPos::new(bx + x, by + y, bz + z));
                            }
                        }
                    }
                }
                true
            }
            SectionFill::Filled => {
                for (i, s) in buf.iter().enumerate() {
                    if wanted.contains(*s) {
                        let i = i as i32;
                        out.push(BlockPos::new(bx + (i & 15), by + (i >> 8), bz + ((i >> 4) & 15)));
                    }
                }
                true
            }
        }
    }
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

/// A section the search has read: one state for all of it, or all 4096.
enum Section {
    Unfetched,
    Uniform(StateId),
    Data(Box<[StateId; 4096]>),
}

/// A chunk column the search has touched.
struct Column {
    /// Loaded in the live world (else blocks come from the cache).
    loaded: bool,
    sections: Box<[Section]>,
}

/// The sections a search has copied, by chunk.
struct SectionCache {
    columns: Vec<Column>,
    index: rustc_hash::FxHashMap<(i32, i32), u32>,
    last_key: (i32, i32),
    last: u32,
    scratch: Box<[StateId; 4096]>,
}

/// `BlockStateInterface`: a block lookup that reads the loaded chunks first, then Baritone's cache.
///
/// For a path search ([`BlockStateInterface::for_search`]), every section it touches is copied once into a
/// flat array, so the millions of reads that follow are plain indexing. Live, one-tick lookups
/// ([`BlockStateInterface::new`]) read the source directly.
pub struct BlockStateInterface {
    pub table: Arc<BlockTable>,
    source: Arc<dyn BlockSource>,
    cache: Option<Arc<CachedWorld>>,
    pub dimension: Arc<Dimension>,
    pub border: WorldBorder,
    use_the_real_world: bool,
    min_y: i32,
    height: i32,
    sections_per_column: i32,
    air: StateId,
    sections: Option<std::cell::RefCell<SectionCache>>,
    prev_cached: std::cell::Cell<Option<(i32, i32)>>,
    prev_region: std::cell::RefCell<Option<Arc<CachedRegion>>>,
}

impl BlockStateInterface {
    /// A lookup reading the source directly; `use_the_real_world` is `!pathThroughCachedOnly`.
    pub fn new(view: &WorldView, use_the_real_world: bool) -> Self {
        Self::with_options(view, use_the_real_world, false)
    }

    /// A lookup for a path search, which copies the sections it reads.
    pub fn for_search(view: &WorldView, use_the_real_world: bool) -> Self {
        Self::with_options(view, use_the_real_world, true)
    }

    fn with_options(view: &WorldView, use_the_real_world: bool, cache_sections: bool) -> Self {
        let height = view.dimension.height.max(16);
        Self {
            table: view.table.clone(),
            source: view.source.clone(),
            cache: view.cache.clone(),
            dimension: view.dimension.clone(),
            border: view.border,
            use_the_real_world,
            min_y: view.dimension.min_y,
            height,
            sections_per_column: (height + 15) >> 4,
            air: view.table.air(),
            sections: cache_sections.then(|| {
                std::cell::RefCell::new(SectionCache {
                    columns: Vec::new(),
                    index: rustc_hash::FxHashMap::default(),
                    last_key: (i32::MIN, i32::MIN),
                    last: 0,
                    scratch: Box::new([StateId(0); 4096]),
                })
            }),
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
        match &self.sections {
            Some(cache) => {
                let mut c = cache.borrow_mut();
                let i = self.column(&mut c, x >> 4, z >> 4);
                c.columns[i as usize].loaded
            }
            None => self.source.is_loaded(x, z),
        }
    }

    #[inline]
    pub fn get_pos(&self, pos: BlockPos) -> StateId {
        self.get(pos.x, pos.y, pos.z)
    }

    /// The column's index in the cache, adding it (and asking the source whether it is loaded) the first time.
    #[inline]
    fn column(&self, c: &mut SectionCache, cx: i32, cz: i32) -> u32 {
        if c.last_key == (cx, cz) {
            return c.last;
        }
        let i = match c.index.get(&(cx, cz)) {
            Some(i) => *i,
            None => {
                let loaded = self.source.is_loaded(cx << 4, cz << 4);
                let i = c.columns.len() as u32;
                c.columns.push(Column {
                    loaded,
                    sections: (0..self.sections_per_column)
                        .map(|_| Section::Unfetched)
                        .collect(),
                });
                c.index.insert((cx, cz), i);
                i
            }
        };
        c.last_key = (cx, cz);
        c.last = i;
        i
    }

    /// `get0`: the loaded world if the chunk is there, else the cached region, else air.
    #[inline]
    pub fn get(&self, x: i32, y: i32, z: i32) -> StateId {
        let adj = y - self.min_y;
        if adj < 0 || adj >= self.height {
            return self.air;
        }
        let Some(cache) = &self.sections else {
            if self.use_the_real_world && self.source.is_loaded(x, z) {
                return self.source.state(x, y, z);
            }
            return self.get_cached(x, y, z);
        };
        let mut c = cache.borrow_mut();
        let ci = self.column(&mut c, x >> 4, z >> 4) as usize;
        if !(self.use_the_real_world && c.columns[ci].loaded) {
            drop(c);
            return self.get_cached(x, y, z);
        }
        let sy = (adj >> 4) as usize;
        let idx = ((adj & 15) << 8 | (z & 15) << 4 | (x & 15)) as usize;
        match &c.columns[ci].sections[sy] {
            Section::Uniform(s) => return *s,
            Section::Data(d) => return d[idx],
            Section::Unfetched => {}
        }
        // The first read of this section: copy all of it.
        let c = &mut *c;
        let fill = self
            .source
            .copy_section(x >> 4, sy as i32, z >> 4, self.min_y, &mut c.scratch);
        let (section, state) = match fill {
            SectionFill::Uniform(s) => (Section::Uniform(s), s),
            SectionFill::Filled => {
                let data = std::mem::replace(&mut c.scratch, Box::new([StateId(0); 4096]));
                let s = data[idx];
                (Section::Data(data), s)
            }
            SectionFill::Unloaded => (Section::Uniform(self.air), self.air),
        };
        c.columns[ci].sections[sy] = section;
        state
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
        if self.world_contains_loaded_chunk(x, z) {
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

    /// How many sections this lookup has copied (for statistics).
    pub fn sections_copied(&self) -> usize {
        self.sections.as_ref().map_or(0, |c| {
            c.borrow()
                .columns
                .iter()
                .map(|col| {
                    col.sections
                        .iter()
                        .filter(|s| !matches!(s, Section::Unfetched))
                        .count()
                })
                .sum()
        })
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
