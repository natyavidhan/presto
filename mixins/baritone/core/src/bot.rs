//! The parts of Baritone the processes and commands share: settings, behaviors, the world's data and what
//! Baritone has to say. (`Baritone` adds the processes and the control manager around it.)

use crate::behavior::input::InputOverrideHandler;
use crate::behavior::inventory::InventoryBehavior;
use crate::behavior::look::LookBehavior;
use crate::behavior::pathing::PathingBehavior;
use crate::cache::waypoints::WaypointCollection;
use crate::cache::{CacheConfig, CachedWorld};
use crate::settings::Settings;
use crate::utils::output::Output;
use crate::world::{BlockTable, Dimension};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// `WorldData`: one world and dimension's cache and waypoints.
pub struct WorldData {
    pub directory: PathBuf,
    pub cache: Arc<CachedWorld>,
    pub waypoints: WaypointCollection,
    pub dimension: Arc<Dimension>,
}

impl WorldData {
    fn open(directory: PathBuf, dimension: Arc<Dimension>, table: Arc<BlockTable>, config: CacheConfig) -> Self {
        let _ = std::fs::create_dir_all(&directory);
        Self {
            cache: CachedWorld::open(directory.join("cache"), dimension.clone(), table, config),
            waypoints: WaypointCollection::open(directory.join("waypoints")),
            directory,
            dimension,
        }
    }
}

/// `WorldProvider`: the world data of the world the player is in.
#[derive(Default)]
pub struct WorldProvider {
    current: Option<WorldData>,
    key: Option<(PathBuf, String, u64)>,
}

impl WorldProvider {
    pub fn current(&self) -> Option<&WorldData> {
        self.current.as_ref()
    }
    pub fn current_mut(&mut self) -> Option<&mut WorldData> {
        self.current.as_mut()
    }

    /// `getWorldDataDirectory`: `<world>/<namespace>/<path>_<logical height>`.
    pub fn world_data_directory(world_dir: &Path, dim: &Dimension) -> PathBuf {
        let (ns, path) = dim.id.split_once(':').unwrap_or(("minecraft", &dim.id));
        world_dir.join(ns).join(format!("{}_{}", path.replace('/', "_"), dim.logical_height))
    }

    /// Opens the world data for this world and dimension if it isn't the one open (`initWorld`), closing
    /// the old one. Returns true when it changed.
    pub fn ensure(
        &mut self,
        world_dir: &Path,
        dim: &Arc<Dimension>,
        table: &Arc<BlockTable>,
        config: CacheConfig,
    ) -> bool {
        let key = (world_dir.to_path_buf(), dim.id.clone(), table.generation);
        if self.key.as_ref() == Some(&key) {
            return false;
        }
        self.close_world();
        // The readme Java Baritone leaves in its folder.
        let _ = std::fs::create_dir_all(world_dir);
        let _ = std::fs::write(world_dir.join("readme.txt"), "https://github.com/cabaletta/baritone\n");
        let dir = Self::world_data_directory(world_dir, dim);
        log::info!("[Baritone] world data dir: {}", dir.display());
        self.current = Some(WorldData::open(dir, dim.clone(), table.clone(), config));
        self.key = Some(key);
        true
    }

    /// `closeWorld`: saves the cache in the background.
    pub fn close_world(&mut self) {
        if let Some(w) = self.current.take() {
            w.cache.close();
        }
        self.key = None;
    }
}

/// `InventoryPauserProcess`'s state, which `InventoryBehavior` reads (`stationaryForInventoryMove`).
#[derive(Debug, Default, Clone, Copy)]
pub struct PauserState {
    pub pause_requested_last_tick: bool,
    pub safe_to_cancel_last_tick: bool,
    pub ticks_of_stationary: i32,
}

impl PauserState {
    /// `stationaryForInventoryMove`'s answer. Asking it also asks for a pause, which the tick passes on from
    /// [`InventoryBehavior::pause_requested`](crate::behavior::inventory::InventoryBehavior::pause_requested).
    pub fn stationary(&self) -> bool {
        self.safe_to_cancel_last_tick && self.ticks_of_stationary > 1
    }
}

/// Everything but the processes.
pub struct Bot {
    pub settings: Arc<Settings>,
    pub look: LookBehavior,
    pub input: InputOverrideHandler,
    pub inventory: InventoryBehavior,
    pub pathing: PathingBehavior,
    pub world: WorldProvider,
    pub out: Output,
    pub pauser: PauserState,
    /// Baritone's folder (`.minecraft/baritone`), where `settings.txt` lives.
    pub directory: PathBuf,
}

impl Bot {
    pub fn new(directory: PathBuf, settings: Settings) -> Self {
        Self {
            settings: Arc::new(settings),
            look: LookBehavior::default(),
            input: InputOverrideHandler::default(),
            inventory: InventoryBehavior::default(),
            pathing: PathingBehavior::default(),
            world: WorldProvider::default(),
            out: Output::default(),
            pauser: PauserState::default(),
            directory,
        }
    }

    pub fn log_direct(&mut self, msg: &str) {
        let s = self.settings.clone();
        self.out.log_direct(&s, msg);
    }

    pub fn log_colored(&mut self, msg: &str, color: u32) {
        let s = self.settings.clone();
        self.out.log_colored(&s, msg, color);
    }

    pub fn log_debug(&mut self, msg: &str) {
        let s = self.settings.clone();
        self.out.log_debug(&s, msg);
    }

    pub fn log_notification(&mut self, msg: &str, error: bool) {
        let s = self.settings.clone();
        self.out.log_notification(&s, msg, error);
    }

    /// Changes settings: the next search and everything after reads the new values.
    pub fn settings_mut(&mut self) -> &mut Settings {
        Arc::make_mut(&mut self.settings)
    }

    /// Writes `settings.txt` (`SettingsUtil.save`).
    pub fn save_settings(&self) {
        let path = self.directory.join("settings.txt");
        let _ = std::fs::create_dir_all(&self.directory);
        if let Err(e) = std::fs::write(&path, self.settings.to_file()) {
            log::warn!("[Baritone] couldn't save settings: {e}");
        }
    }
}
