//! `CalculationContext` and `ToolSet`: everything a path calculation reads, copied out of the game and the
//! settings when it starts so the whole path is costed one consistent way, and safe to move to the thread
//! that runs it.

use super::costs::{COST_INF, WALK_ONE_BLOCK_COST, WALK_ONE_IN_WATER_COST};
use super::precomputed::PrecomputedData;
use crate::math::BlockPos;
use crate::player::{GameState, Inventory, ItemStack, PlayerState};
use crate::settings::Settings;
use crate::world::{BlockId, BlockInfo, BlockStateInterface, BlockTable, StateId, WorldBorder, WorldView};
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;
use std::sync::Arc;

/// `ToolSet`: how fast the best hotbar tool breaks each block.
pub struct ToolSet {
    hotbar: Vec<ItemStack>,
    selected: usize,
    cache: RefCell<FxHashMap<BlockId, f64>>,
    amplifier: Option<f64>,
    auto_tool: bool,
    use_sword_to_mine: bool,
    item_saver: bool,
    item_saver_threshold: i32,
    blocks_to_avoid_breaking: FxHashSet<BlockId>,
    avoid_breaking_multiplier: f64,
}

impl ToolSet {
    pub fn new(inv: &Inventory, player: Option<&PlayerState>, settings: &Settings, table: &BlockTable) -> Self {
        let amplifier = settings
            .consider_potion_effects
            .then(|| player.map(Self::potion_amplifier).unwrap_or(1.0));
        Self {
            hotbar: (0..9).map(|i| inv.get(i).clone()).collect(),
            selected: inv.selected,
            cache: RefCell::new(FxHashMap::default()),
            amplifier,
            auto_tool: settings.auto_tool,
            use_sword_to_mine: settings.use_sword_to_mine,
            item_saver: settings.item_saver,
            item_saver_threshold: settings.item_saver_threshold,
            blocks_to_avoid_breaking: settings
                .blocks_to_avoid_breaking
                .iter()
                .filter_map(|n| table.block_id(n))
                .collect(),
            avoid_breaking_multiplier: settings.avoid_breaking_multiplier,
        }
    }

    /// `getStrVsBlock`: the best hotbar tool's breaking speed, per tick (1 / ticks to break).
    pub fn str_vs_block(&self, info: &BlockInfo) -> f64 {
        if let Some(v) = self.cache.borrow().get(&info.block) {
            return *v;
        }
        let slot = self.best_slot(info, false, true);
        let mut v = Self::calculate_speed_vs_block(&self.hotbar[slot], info);
        if self.blocks_to_avoid_breaking.contains(&info.block) {
            v *= self.avoid_breaking_multiplier;
        }
        if let Some(a) = self.amplifier {
            v *= a;
        }
        self.cache.borrow_mut().insert(info.block, v);
        v
    }

    /// `getBestSlot`.
    pub fn best_slot(&self, info: &BlockInfo, prefer_silk_touch: bool, pathing_calculation: bool) -> usize {
        // With auto tool off, paths are costed with the tool that will actually be used.
        if !self.auto_tool && pathing_calculation {
            return self.selected;
        }
        let mut best = 0;
        let mut highest = f64::NEG_INFINITY;
        let mut lowest_cost = i32::MIN;
        let mut best_silk = false;
        for (i, stack) in self.hotbar.iter().enumerate() {
            if !self.use_sword_to_mine && stack.is_sword {
                continue;
            }
            if self.item_saver
                && stack.damage + self.item_saver_threshold >= stack.max_damage
                && stack.max_damage > 1
            {
                continue;
            }
            let speed = Self::calculate_speed_vs_block(stack, info);
            let silk = stack.silk_touch;
            if speed > highest {
                highest = speed;
                best = i;
                lowest_cost = stack.material_cost;
                best_silk = silk;
            } else if speed == highest {
                let cost = stack.material_cost;
                if (cost < lowest_cost && (silk || !best_silk))
                    || (prefer_silk_touch && !best_silk && silk)
                {
                    highest = speed;
                    best = i;
                    lowest_cost = cost;
                    best_silk = silk;
                }
            }
        }
        best
    }

    /// `calculateSpeedVsBlock`: an item's breaking speed against a block, per tick; -1 if unbreakable.
    pub fn calculate_speed_vs_block(item: &ItemStack, info: &BlockInfo) -> f64 {
        let hardness = info.hardness;
        if hardness < 0.0 {
            return -1.0;
        }
        let mut speed = if item.is_empty() {
            1.0
        } else {
            item.speeds.as_ref().map_or(1.0, |s| s.speed(info.block))
        };
        if speed > 1.0 && !item.is_empty() {
            speed += item.mining_efficiency;
        }
        let speed = speed as f64 / hardness as f64;
        let correct = !item.is_empty()
            && item
                .speeds
                .as_ref()
                .is_some_and(|s| s.correct(info.block));
        if !info.requires_tool || correct {
            speed / 30.0
        } else {
            speed / 100.0
        }
    }

    /// `potionAmplifier`: Haste and Mining Fatigue.
    pub fn potion_amplifier(player: &PlayerState) -> f64 {
        let mut speed = 1.0;
        if let Some(a) = player.haste {
            speed *= 1.0 + (a + 1) as f64 * 0.2;
        }
        if let Some(a) = player.mining_fatigue {
            speed *= match a {
                0 => 0.3,
                1 => 0.09,
                // 0.09 * 0.3 would be 0.027; vanilla's table says 0.0027.
                2 => 0.0027,
                _ => 0.00081,
            };
        }
        speed
    }
}

/// `CalculationContext`.
pub struct CalculationContext {
    pub table: Arc<BlockTable>,
    pub bsi: BlockStateInterface,
    pub precomputed: PrecomputedData,
    pub tool_set: ToolSet,
    pub settings: Arc<Settings>,
    pub safe_for_threaded_use: bool,
    pub has_water_bucket: bool,
    pub has_throwaway: bool,
    pub can_sprint: bool,
    place_block_cost: f64,
    pub allow_break: bool,
    pub allow_break_anyway: FxHashSet<BlockId>,
    pub allow_parkour: bool,
    pub allow_parkour_place: bool,
    pub allow_jump_at_build_limit: bool,
    pub allow_parkour_ascend: bool,
    pub assume_walk_on_water: bool,
    /// The secret setting `ElytraBehavior` turns on.
    pub allow_fall_into_lava: bool,
    pub frost_walker: i32,
    pub allow_diagonal_descend: bool,
    pub allow_diagonal_ascend: bool,
    pub allow_downward: bool,
    pub min_fall_height: i32,
    pub max_fall_height_no_water: i32,
    pub max_fall_height_bucket: i32,
    pub water_walk_speed: f64,
    pub break_block_additional_cost: f64,
    pub backtrack_cost_favoring_coefficient: f64,
    pub jump_penalty: f64,
    pub walk_on_water_one_penalty: f64,
    pub allow_walk_on_magma_blocks: bool,
    pub world_border: WorldBorder,
    pub blocks_to_avoid: FxHashSet<BlockId>,
    pub blocks_to_disallow_breaking: FxHashSet<BlockId>,
    /// `world.getMinY()`.
    pub min_y: i32,
    /// `world.getMaxY()`: the top layer blocks can be at.
    pub max_y: i32,
    /// Overrides of what a position costs to break or place at (the builder's and the selection's).
    pub protection: Option<Arc<dyn Protection>>,
}

/// Positions a process wants left alone or treated specially while costing paths (`isPossiblyProtected`, and
/// the builder's overrides of `costOfPlacingAt` and `breakCostMultiplierAt`).
pub trait Protection: Send + Sync {
    /// `None` to use the default, else the cost to place a block here (`COST_INF` for never).
    fn place_cost(&self, _x: i32, _y: i32, _z: i32, _current: &BlockInfo) -> Option<f64> {
        None
    }
    /// `None` to use the default, else the multiplier for breaking here (`COST_INF` for never).
    fn break_multiplier(&self, _x: i32, _y: i32, _z: i32, _current: &BlockInfo) -> Option<f64> {
        None
    }
}

impl CalculationContext {
    /// A context for the world as it is now. `None` outside a world.
    pub fn new(
        game: &GameState,
        settings: &Arc<Settings>,
        has_generic_throwaway: bool,
        for_use_on_another_thread: bool,
    ) -> Option<CalculationContext> {
        let view = game.world.as_ref()?;
        let player = game.player.as_ref();
        Some(Self::from_parts(
            view,
            &game.inventory,
            player,
            settings,
            has_generic_throwaway,
            for_use_on_another_thread,
        ))
    }

    pub fn from_parts(
        view: &WorldView,
        inventory: &Inventory,
        player: Option<&PlayerState>,
        settings: &Arc<Settings>,
        has_generic_throwaway: bool,
        for_use_on_another_thread: bool,
    ) -> CalculationContext {
        let s = settings.as_ref();
        let table = view.table.clone();
        // A context for a search copies the sections it reads into flat arrays; a live one reads directly.
        let bsi = if for_use_on_another_thread {
            BlockStateInterface::for_search(view, !s.path_through_cached_only)
        } else {
            BlockStateInterface::new(view, !s.path_through_cached_only)
        };
        let precomputed = PrecomputedData::new(&table);
        let tool_set = ToolSet::new(inventory, player, s, &table);
        let has_water_bucket = s.allow_water_bucket_fall
            && inventory.find_in_hotbar("minecraft:water_bucket").is_some()
            && !view.dimension.is_nether();
        let can_sprint = s.allow_sprint && player.is_none_or(|p| p.food > 6);
        let frost_walker = inventory
            .armor
            .iter()
            .chain(std::iter::once(&inventory.offhand))
            .chain(std::iter::once(inventory.held()))
            .map(|i| i.frost_walker)
            .max()
            .unwrap_or(0);
        // Depth Strider's share of land speed in water (1/3 a level). Without it this is 0, as in Baritone
        // before 1.21; the 1.21 port starts it at 1, which costs water like land when there is no Depth Strider.
        let water_speed_multiplier = inventory
            .armor
            .iter()
            .map(|i| i.water_movement_efficiency)
            .find(|v| *v > 0.0)
            .unwrap_or(0.0)
            .min(1.0) as f64;
        let ids = |list: &crate::settings::BlockList| -> FxHashSet<BlockId> {
            list.iter().filter_map(|n| table.block_id(n)).collect()
        };
        CalculationContext {
            safe_for_threaded_use: for_use_on_another_thread,
            has_water_bucket,
            has_throwaway: s.allow_place && has_generic_throwaway,
            can_sprint,
            place_block_cost: s.block_placement_penalty,
            allow_break: s.allow_break,
            allow_break_anyway: ids(&s.allow_break_anyway),
            allow_parkour: s.allow_parkour,
            allow_parkour_place: s.allow_parkour_place,
            allow_jump_at_build_limit: s.allow_jump_at_build_limit,
            allow_parkour_ascend: s.allow_parkour_ascend,
            assume_walk_on_water: s.assume_walk_on_water,
            allow_fall_into_lava: false,
            frost_walker,
            allow_diagonal_descend: s.allow_diagonal_descend,
            allow_diagonal_ascend: s.allow_diagonal_ascend,
            allow_downward: s.allow_downward,
            min_fall_height: 3,
            max_fall_height_no_water: s.max_fall_height_no_water,
            max_fall_height_bucket: s.max_fall_height_bucket,
            water_walk_speed: WALK_ONE_IN_WATER_COST * (1.0 - water_speed_multiplier)
                + WALK_ONE_BLOCK_COST * water_speed_multiplier,
            break_block_additional_cost: s.block_break_additional_penalty,
            backtrack_cost_favoring_coefficient: s.backtrack_cost_favoring_coefficient,
            jump_penalty: s.jump_penalty,
            walk_on_water_one_penalty: s.walk_on_water_one_penalty,
            allow_walk_on_magma_blocks: s.allow_walk_on_magma_blocks,
            world_border: view.border,
            blocks_to_avoid: ids(&s.blocks_to_avoid),
            blocks_to_disallow_breaking: ids(&s.blocks_to_disallow_breaking),
            min_y: view.dimension.min_y,
            max_y: view.dimension.max_y() - 1,
            protection: None,
            settings: settings.clone(),
            table,
            bsi,
            precomputed,
            tool_set,
        }
    }

    #[inline]
    pub fn get(&self, x: i32, y: i32, z: i32) -> StateId {
        self.bsi.get(x, y, z)
    }

    #[inline]
    pub fn get_pos(&self, pos: BlockPos) -> StateId {
        self.bsi.get(pos.x, pos.y, pos.z)
    }

    #[inline]
    pub fn info(&self, x: i32, y: i32, z: i32) -> &BlockInfo {
        self.table.get(self.bsi.get(x, y, z))
    }

    #[inline]
    pub fn info_pos(&self, pos: BlockPos) -> &BlockInfo {
        self.info(pos.x, pos.y, pos.z)
    }

    pub fn is_loaded(&self, x: i32, z: i32) -> bool {
        self.bsi.is_loaded(x, z)
    }

    /// `costOfPlacingAt`.
    pub fn cost_of_placing_at(&self, x: i32, y: i32, z: i32, current: &BlockInfo) -> f64 {
        if !self.has_throwaway {
            return COST_INF;
        }
        if let Some(p) = &self.protection
            && let Some(c) = p.place_cost(x, y, z, current)
        {
            return c;
        }
        if self.is_possibly_protected(x, y, z) {
            return COST_INF;
        }
        if !self.world_border.can_place_at(x, z) {
            return COST_INF;
        }
        if !self.settings.allow_place_in_fluids_source && current.has_fluid() && current.fluid_source {
            return COST_INF;
        }
        if !self.settings.allow_place_in_fluids_flow && current.has_fluid() && !current.fluid_source {
            return COST_INF;
        }
        self.place_block_cost
    }

    /// `breakCostMultiplierAt`.
    pub fn break_cost_multiplier_at(&self, x: i32, y: i32, z: i32, current: &BlockInfo) -> f64 {
        if !self.allow_break && !self.allow_break_anyway.contains(&current.block) {
            return COST_INF;
        }
        if let Some(p) = &self.protection
            && let Some(m) = p.break_multiplier(x, y, z, current)
        {
            return m;
        }
        if self.is_possibly_protected(x, y, z) {
            return COST_INF;
        }
        1.0
    }

    pub fn place_bucket_cost(&self) -> f64 {
        self.place_block_cost
    }

    /// `isPossiblyProtected`: nothing yet, as in Baritone (#220).
    pub fn is_possibly_protected(&self, _x: i32, _y: i32, _z: i32) -> bool {
        false
    }
}
