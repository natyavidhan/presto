//! Baritone's memory of the world beyond the loaded chunks (`baritone.cache`): every chunk the client sees is
//! packed to two bits a block on a worker thread and kept in regions, saved to disk, so paths can be planned
//! through terrain that has unloaded and `#mine` can go back to ores it saw.

pub mod chunk;
pub mod region;
pub mod waypoints;

pub use chunk::{CachedChunk, PathingBlockType};
pub use region::CachedRegion;

use crate::math::BlockPos;
use crate::world::{BlockSource, BlockTable, Dimension};
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, Weak};
use std::time::{Duration, Instant};

const REGION_MAX: i32 = 30_000_000 / 512 + 1;

/// The settings the cache reads, copied in by Baritone whenever they may have changed.
#[derive(Clone, Copy, Debug)]
pub struct CacheConfig {
    pub chunk_caching: bool,
    pub cached_chunks_expiry_seconds: i64,
    pub prune_regions_from_ram: bool,
    pub chunk_packer_queue_max_size: i32,
    pub allow_walk_on_magma_blocks: bool,
}

impl Default for CacheConfig {
    fn default() -> Self {
        let s = crate::settings::Settings::default();
        Self::from_settings(&s)
    }
}

impl CacheConfig {
    pub fn from_settings(s: &crate::settings::Settings) -> Self {
        Self {
            chunk_caching: s.chunk_caching,
            cached_chunks_expiry_seconds: s.cached_chunks_expiry_seconds,
            prune_regions_from_ram: s.prune_regions_from_ram,
            chunk_packer_queue_max_size: s.chunk_packer_queue_max_size,
            allow_walk_on_magma_blocks: s.allow_walk_on_magma_blocks,
        }
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

struct PackQueue {
    order: VecDeque<(i32, i32)>,
    pending: FxHashMap<(i32, i32), Arc<dyn BlockSource>>,
}

/// `CachedWorld`: one dimension's regions, the packer and the periodic save.
pub struct CachedWorld {
    directory: PathBuf,
    pub dimension: Arc<Dimension>,
    table: Arc<BlockTable>,
    regions: Mutex<FxHashMap<(i32, i32), Arc<CachedRegion>>>,
    dirty: Mutex<FxHashSet<(i32, i32)>>,
    queue: Mutex<PackQueue>,
    wake: Condvar,
    config: Mutex<CacheConfig>,
    /// Where the player is, for pruning far regions out of memory.
    player: Mutex<Option<BlockPos>>,
    closed: AtomicBool,
}

impl CachedWorld {
    /// Opens the cache kept in `directory` and starts its worker thread.
    pub fn open(
        directory: PathBuf,
        dimension: Arc<Dimension>,
        table: Arc<BlockTable>,
        config: CacheConfig,
    ) -> Arc<CachedWorld> {
        let _ = std::fs::create_dir_all(&directory);
        let world = Arc::new(CachedWorld {
            directory,
            dimension,
            table,
            regions: Mutex::new(FxHashMap::default()),
            dirty: Mutex::new(FxHashSet::default()),
            queue: Mutex::new(PackQueue {
                order: VecDeque::new(),
                pending: FxHashMap::default(),
            }),
            wake: Condvar::new(),
            config: Mutex::new(config),
            player: Mutex::new(None),
            closed: AtomicBool::new(false),
        });
        let weak = Arc::downgrade(&world);
        let _ = std::thread::Builder::new()
            .name("Baritone cache".into())
            .spawn(move || Self::worker(weak));
        world
    }

    /// A cache with no directory and no thread, for tests and tools.
    pub fn in_memory(dimension: Arc<Dimension>, table: Arc<BlockTable>) -> Arc<CachedWorld> {
        Arc::new(CachedWorld {
            directory: PathBuf::new(),
            dimension,
            table,
            regions: Mutex::new(FxHashMap::default()),
            dirty: Mutex::new(FxHashSet::default()),
            queue: Mutex::new(PackQueue {
                order: VecDeque::new(),
                pending: FxHashMap::default(),
            }),
            wake: Condvar::new(),
            config: Mutex::new(CacheConfig {
                chunk_caching: false,
                ..CacheConfig::default()
            }),
            player: Mutex::new(None),
            closed: AtomicBool::new(true),
        })
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn table(&self) -> &Arc<BlockTable> {
        &self.table
    }

    pub fn set_config(&self, config: CacheConfig) {
        *self.config.lock().unwrap_or_else(|e| e.into_inner()) = config;
    }

    pub fn set_player_position(&self, pos: BlockPos) {
        *self.player.lock().unwrap_or_else(|e| e.into_inner()) = Some(pos);
    }

    fn worker(world: Weak<CachedWorld>) {
        let started = Instant::now();
        let mut last_save = Instant::now();
        let mut first_save_done = false;
        loop {
            let Some(w) = world.upgrade() else { return };
            if w.closed.load(Ordering::Acquire) {
                return;
            }
            let next = {
                let mut q = w.queue.lock().unwrap_or_else(|e| e.into_inner());
                if q.order.is_empty() {
                    let (guard, _) = w
                        .wake
                        .wait_timeout(q, Duration::from_millis(500))
                        .unwrap_or_else(|e| e.into_inner());
                    q = guard;
                }
                match q.order.pop_front() {
                    Some(pos) => {
                        let max = w
                            .config
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .chunk_packer_queue_max_size;
                        let src = q.pending.remove(&pos);
                        // `PackerThread`: past the limit, chunks are dropped rather than packed.
                        if q.order.len() > max.max(0) as usize {
                            None
                        } else {
                            src.map(|s| (pos, s))
                        }
                    }
                    None => None,
                }
            };
            if let Some(((cx, cz), src)) = next {
                let magma = w
                    .config
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .allow_walk_on_magma_blocks;
                let packed = chunk::pack(&*src, &w.table, &w.dimension, cx, cz, magma, now_ms());
                w.update_cached_chunk(packed);
            }
            // Save 30 seconds in, then every 10 minutes (only changed regions are written).
            let due = if first_save_done {
                last_save.elapsed() >= Duration::from_secs(600)
            } else {
                started.elapsed() >= Duration::from_secs(30)
            };
            if due {
                first_save_done = true;
                last_save = Instant::now();
                w.save();
            }
        }
    }

    /// Queues a loaded chunk to be packed (`queueForPacking`). `source` must have the chunk loaded.
    pub fn queue_for_packing(&self, cx: i32, cz: i32, source: Arc<dyn BlockSource>) {
        if self.closed.load(Ordering::Acquire) {
            return;
        }
        let mut q = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        if q.pending.insert((cx, cz), source).is_none() {
            q.order.push_back((cx, cz));
        }
        drop(q);
        self.wake.notify_one();
    }

    /// Packs a chunk now, on this thread.
    pub fn pack_now(&self, cx: i32, cz: i32, source: &dyn BlockSource) {
        let magma = self
            .config
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .allow_walk_on_magma_blocks;
        let packed = chunk::pack(source, &self.table, &self.dimension, cx, cz, magma, now_ms());
        self.update_cached_chunk(packed);
    }

    fn update_cached_chunk(&self, chunk: CachedChunk) {
        let (rx, rz) = (chunk.x >> 5, chunk.z >> 5);
        let (cx, cz) = (chunk.x & 31, chunk.z & 31);
        let region = self.get_or_create_region(rx, rz);
        let next = Arc::new(region.with_chunk(cx, cz, Some(Arc::new(chunk))));
        self.regions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert((rx, rz), next);
        self.dirty
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert((rx, rz));
    }

    fn in_world(rx: i32, rz: i32) -> bool {
        (-REGION_MAX..=REGION_MAX).contains(&rx) && (-REGION_MAX..=REGION_MAX).contains(&rz)
    }

    /// The region if it is in memory (`getRegion`).
    pub fn region(&self, rx: i32, rz: i32) -> Option<Arc<CachedRegion>> {
        if !Self::in_world(rx, rz) {
            return None;
        }
        self.regions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&(rx, rz))
            .cloned()
    }

    /// The region, read from disk the first time (`getOrCreateRegion`).
    pub fn get_or_create_region(&self, rx: i32, rz: i32) -> Arc<CachedRegion> {
        if let Some(r) = self.region(rx, rz) {
            return r;
        }
        let loaded = if self.directory.as_os_str().is_empty() {
            None
        } else {
            match CachedRegion::load(&self.directory, rx, rz, self.dimension.clone(), &self.table) {
                Ok(r) => r,
                Err(e) => {
                    log::warn!("[Baritone] couldn't read cached region {rx},{rz}: {e}");
                    None
                }
            }
        };
        let expiry = self
            .config
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .cached_chunks_expiry_seconds;
        let region = loaded
            .map(|r| r.without_expired(expiry, now_ms()).unwrap_or(r))
            .unwrap_or_else(|| CachedRegion::empty(rx, rz, self.dimension.clone()));
        let region = Arc::new(region);
        let mut regions = self.regions.lock().unwrap_or_else(|e| e.into_inner());
        regions.entry((rx, rz)).or_insert(region).clone()
    }

    /// Loads a region from disk if it isn't in memory yet (`tryLoadFromDisk`).
    pub fn try_load_from_disk(&self, rx: i32, rz: i32) {
        let _ = self.get_or_create_region(rx, rz);
    }

    /// Whether a column is cached (`isCached`).
    pub fn is_cached(&self, x: i32, z: i32) -> bool {
        self.region(x >> 9, z >> 9)
            .is_some_and(|r| r.is_cached(x & 511, z & 511))
    }

    pub fn region_loaded(&self, x: i32, z: i32) -> bool {
        self.region(x >> 9, z >> 9).is_some()
    }

    /// Where the cache remembers blocks of one kind, in rings of regions around a point (`getLocationsOf`).
    pub fn locations_of(
        &self,
        block: &str,
        maximum: usize,
        center_x: i32,
        center_z: i32,
        max_region_distance_sq: i32,
    ) -> Vec<BlockPos> {
        let mut res = Vec::new();
        let (crx, crz) = (center_x >> 9, center_z >> 9);
        let mut radius = 0;
        while radius <= max_region_distance_sq {
            for xoff in -radius..=radius {
                for zoff in -radius..=radius {
                    let distance = xoff * xoff + zoff * zoff;
                    if distance != radius {
                        continue;
                    }
                    let (rx, rz) = (xoff + crx, zoff + crz);
                    if !Self::in_world(rx, rz) {
                        continue;
                    }
                    res.extend(self.get_or_create_region(rx, rz).locations_of(block));
                }
            }
            if res.len() >= maximum {
                return res;
            }
            radius += 1;
        }
        res
    }

    /// Writes every changed region (`save`), then forgets far ones when `pruneRegionsFromRAM` is on.
    pub fn save(&self) {
        let config = *self.config.lock().unwrap_or_else(|e| e.into_inner());
        // Expired chunks leave memory whether or not they are saved.
        {
            let mut regions = self.regions.lock().unwrap_or_else(|e| e.into_inner());
            let now = now_ms();
            for r in regions.values_mut() {
                if let Some(next) = r.without_expired(config.cached_chunks_expiry_seconds, now) {
                    *r = Arc::new(next);
                }
            }
        }
        if config.chunk_caching && !self.directory.as_os_str().is_empty() {
            let dirty: Vec<(i32, i32)> = self
                .dirty
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .drain()
                .collect();
            for key in dirty {
                if let Some(region) = self.region(key.0, key.1)
                    && let Err(e) = region.save(&self.directory)
                {
                    log::warn!("[Baritone] couldn't save cached region {key:?}: {e}");
                    self.dirty
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .insert(key);
                }
            }
        }
        if config.prune_regions_from_ram {
            self.prune();
        }
    }

    fn prune(&self) {
        let center = self
            .player
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .or_else(|| self.guess_position())
            .unwrap_or(BlockPos::ORIGIN);
        let dirty = self.dirty.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let mut regions = self.regions.lock().unwrap_or_else(|e| e.into_inner());
        regions.retain(|&(rx, rz), _| {
            let dx = ((rx << 9) + 256 - center.x) as f64;
            let dz = ((rz << 9) + 256 - center.z) as f64;
            // A region that still has to be written stays until it is.
            (dx * dx + dz * dz).sqrt() <= 1024.0 || dirty.contains(&(rx, rz))
        });
    }

    fn guess_position(&self) -> Option<BlockPos> {
        let regions = self.regions.lock().unwrap_or_else(|e| e.into_inner());
        regions
            .values()
            .filter_map(|r| r.most_recently_modified().cloned())
            .max_by_key(|c| c.cache_timestamp)
            .map(|c| BlockPos::new((c.x << 4) + 8, 0, (c.z << 4) + 8))
    }

    /// Reads every region in memory from disk again (`reloadAllFromDisk`).
    pub fn reload_all_from_disk(&self) {
        let keys: Vec<(i32, i32)> = self
            .regions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .keys()
            .copied()
            .collect();
        for (rx, rz) in keys {
            if let Ok(Some(r)) =
                CachedRegion::load(&self.directory, rx, rz, self.dimension.clone(), &self.table)
            {
                self.regions
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert((rx, rz), Arc::new(r));
            }
        }
    }

    /// Stops the worker and saves what changed, on a thread of its own so leaving a world doesn't wait.
    pub fn close(self: &Arc<Self>) {
        if self.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        self.wake.notify_all();
        let me = self.clone();
        let _ = std::thread::Builder::new()
            .name("Baritone cache save".into())
            .spawn(move || me.save());
    }

    /// Every region in memory.
    pub fn regions(&self) -> Vec<Arc<CachedRegion>> {
        self.regions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::block::testing;
    use crate::world::{BlockStateInterface, MapSource, WorldBorder, WorldView};

    #[test]
    fn packs_and_reads_back_through_the_interface() {
        let table = Arc::new(testing::vanilla());
        let dim = Arc::new(Dimension::overworld());
        let stone = table.default_state("minecraft:stone").unwrap();
        let chest = table.default_state("minecraft:chest").unwrap();
        let mut src = MapSource {
            default: table.air(),
            ..Default::default()
        };
        for x in 0..16 {
            for z in 0..16 {
                for y in -64..60 {
                    src.set(BlockPos::new(x, y, z), stone);
                }
            }
        }
        src.set(BlockPos::new(3, 60, 4), chest);
        let cache = CachedWorld::in_memory(dim.clone(), table.clone());
        cache.pack_now(0, 0, &src);
        assert!(cache.is_cached(5, 5));
        assert!(!cache.is_cached(16, 5));
        assert_eq!(
            cache.locations_of("minecraft:chest", 10, 0, 0, 1),
            vec![BlockPos::new(3, 60, 4)]
        );
        // Read through a BlockStateInterface whose live source has nothing loaded.
        let view = WorldView {
            table: table.clone(),
            source: Arc::new(crate::world::EmptySource(table.air())),
            cache: Some(cache.clone()),
            dimension: dim,
            border: WorldBorder::default(),
        };
        let bsi = BlockStateInterface::new(&view, true);
        assert!(bsi.info(1, 30, 1).is(crate::world::flags::FULL_COLLISION));
        assert!(bsi.info(1, 70, 1).is_air());
        // The surface block is remembered exactly.
        assert_eq!(bsi.get(1, 59, 1), stone);
        assert_eq!(bsi.get(3, 60, 4), chest);
        assert!(bsi.is_loaded(5, 5));
    }

    #[test]
    fn region_files_round_trip() {
        let table = Arc::new(testing::vanilla());
        let dim = Arc::new(Dimension::overworld());
        let stone = table.default_state("minecraft:stone").unwrap();
        let mut src = MapSource {
            default: table.air(),
            ..Default::default()
        };
        src.set(BlockPos::new(-5, 10, -7), stone);
        let cache = CachedWorld::in_memory(dim.clone(), table.clone());
        cache.pack_now(-1, -1, &src);
        let dir = std::env::temp_dir().join(format!("baritone-cache-test-{}", std::process::id()));
        let region = cache.region(-1, -1).unwrap();
        region.save(&dir).unwrap();
        let back = CachedRegion::load(&dir, -1, -1, dim.clone(), &table)
            .unwrap()
            .unwrap();
        assert_eq!(back.block(511 & -5, 10, 511 & -7, &dim, &table), Some(stone));
        assert!(back.is_cached(511 & -5, 511 & -7));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
