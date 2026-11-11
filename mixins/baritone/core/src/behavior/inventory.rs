//! `InventoryBehavior`: picking throwaway blocks and tools, and (with `allowInventory`) moving them from the
//! main inventory to the hotbar.

use super::look::ForkableRandom;
use crate::pathing::context::ToolSet;
use crate::player::{ClickType, Inventory, InventoryClick, ItemStack};
use crate::settings::Settings;
use crate::world::BlockTable;

#[derive(Debug)]
pub struct InventoryBehavior {
    ticks_since_last_inventory_move: i32,
    /// Remembered while coming to a halt, as not every caller asks every tick.
    last_tick_requested_move: Option<(usize, usize)>,
    /// The hotbar slot chosen this tick.
    pub selected: Option<usize>,
    /// Inventory clicks to make this tick.
    pub clicks: Vec<InventoryClick>,
    /// A move waited this tick for the player to stand still (`stationaryForInventoryMove` was asked), which
    /// asks the inventory pauser to stop the player.
    pub pause_requested: bool,
    rng: ForkableRandom,
}

impl Default for InventoryBehavior {
    fn default() -> Self {
        Self {
            ticks_since_last_inventory_move: 0,
            last_tick_requested_move: None,
            selected: None,
            clicks: Vec::new(),
            pause_requested: false,
            rng: ForkableRandom::from_time(),
        }
    }
}

/// Everything the inventory code reads this tick.
pub struct InvCtx<'a> {
    pub inv: &'a Inventory,
    pub settings: &'a Settings,
    /// A menu other than the player's own inventory is open.
    pub container_open: bool,
    /// `InventoryPauserProcess.stationaryForInventoryMove`.
    pub stationary: bool,
}

impl InventoryBehavior {
    /// Clears what was asked last tick; call before Baritone's tick.
    pub fn begin_tick(&mut self) {
        self.selected = None;
        self.clicks.clear();
        self.pause_requested = false;
    }

    /// The hotbar slot in hand, counting a choice made earlier this tick.
    pub fn selected_slot(&self, inv: &Inventory) -> usize {
        self.selected.unwrap_or(inv.selected)
    }

    pub fn held<'i>(&self, inv: &'i Inventory) -> &'i ItemStack {
        inv.get(self.selected_slot(inv))
    }

    pub fn select(&mut self, slot: usize) {
        self.selected = Some(slot);
    }

    /// `onTick`.
    pub fn on_tick(&mut self, c: &InvCtx, table: &BlockTable) {
        if !c.settings.allow_inventory {
            return;
        }
        if c.container_open {
            return; // a crafting table or a chest is open
        }
        self.ticks_since_last_inventory_move += 1;
        if let Some(t) = first_valid_throwaway(c.inv, c.settings)
            && t >= 9
        {
            // none on the hotbar, but some in the main inventory
            self.request_swap_with_hotbar(c, t, 8);
        }
        if let Some(stone) = table.default_state("minecraft:stone") {
            let pick = best_tool_against(c.inv, c.settings, table.get(stone));
            if let Some(p) = pick
                && p >= 9
            {
                self.request_swap_with_hotbar(c, p, 0);
            }
        }
        if let Some((a, b)) = self.last_tick_requested_move {
            log::debug!("[Baritone] Remembering to move {a} {b} from a previous tick");
            self.request_swap_with_hotbar(c, a, b);
        }
    }

    /// `attemptToPutOnHotbar`.
    pub fn attempt_to_put_on_hotbar(&mut self, c: &InvCtx, in_main: usize, disallowed: impl Fn(usize) -> bool) -> bool {
        match self.get_temp_hotbar_slot(c.inv, disallowed) {
            Some(dest) => self.request_swap_with_hotbar(c, in_main, dest),
            None => true,
        }
    }

    /// `getTempHotbarSlot`: a free hotbar slot between 1 and 7 (0 and 8 are the pickaxe's and the
    /// throwaway's), at random.
    pub fn get_temp_hotbar_slot(&mut self, inv: &Inventory, disallowed: impl Fn(usize) -> bool) -> Option<usize> {
        let mut candidates: Vec<usize> = (1..8)
            .filter(|&i| inv.get(i).is_empty() && !disallowed(i))
            .collect();
        if candidates.is_empty() {
            candidates = (1..8).filter(|&i| !disallowed(i)).collect();
        }
        if candidates.is_empty() {
            return None;
        }
        let i = (self.rng.next_double() * candidates.len() as f64) as usize;
        Some(candidates[i.min(candidates.len() - 1)])
    }

    fn request_swap_with_hotbar(&mut self, c: &InvCtx, in_inventory: usize, in_hotbar: usize) -> bool {
        self.last_tick_requested_move = Some((in_inventory, in_hotbar));
        if self.ticks_since_last_inventory_move < c.settings.ticks_between_inventory_moves {
            log::debug!(
                "[Baritone] Inventory move requested but delaying {} {}",
                self.ticks_since_last_inventory_move,
                c.settings.ticks_between_inventory_moves
            );
            return false;
        }
        if c.settings.inventory_move_only_if_stationary {
            self.pause_requested = true;
            if !c.stationary {
                log::debug!("[Baritone] Inventory move requested but delaying until stationary");
                return false;
            }
        }
        self.clicks.push(InventoryClick {
            slot: if in_inventory < 9 {
                in_inventory as i32 + 36
            } else {
                in_inventory as i32
            },
            button: in_hotbar as i32,
            kind: ClickType::Swap,
        });
        self.ticks_since_last_inventory_move = 0;
        self.last_tick_requested_move = None;
        true
    }

    /// `selectThrowawayForLocation`: a block the builder wants here, else any acceptable throwaway.
    /// `wanted_block` is the block the builder would place at the position, if any.
    pub fn select_throwaway_for_location(&mut self, c: &InvCtx, select: bool, wanted_block: Option<&str>) -> bool {
        if let Some(block) = wanted_block
            && self.throwaway(c, select, |s| s.places_block.as_deref() == Some(block))
        {
            return true;
        }
        for item in c.settings.acceptable_throwaway_items.iter() {
            if self.throwaway(c, select, |s| s.is(item)) {
                return true;
            }
        }
        false
    }

    /// `throwaway`.
    pub fn throwaway(&mut self, c: &InvCtx, select: bool, desired: impl Fn(&ItemStack) -> bool) -> bool {
        self.throwaway_with(c, select, desired, c.settings.allow_inventory)
    }

    pub fn throwaway_with(
        &mut self,
        c: &InvCtx,
        select: bool,
        desired: impl Fn(&ItemStack) -> bool,
        allow_inventory: bool,
    ) -> bool {
        for i in 0..9 {
            if desired(c.inv.get(i)) {
                if select {
                    self.select(i);
                }
                return true;
            }
        }
        if desired(&c.inv.offhand) {
            // The main hand wins a right click, so hold something there that doesn't use itself: an empty hand
            // or a tool.
            for i in 0..9 {
                let item = c.inv.get(i);
                if item.is_empty() || item.is_tool {
                    if select {
                        self.select(i);
                    }
                    return true;
                }
            }
        }
        if allow_inventory {
            for i in 9..36 {
                if desired(c.inv.get(i)) {
                    if select {
                        self.request_swap_with_hotbar(c, i, 7);
                        self.select(7);
                    }
                    return true;
                }
            }
        }
        false
    }
}

/// `hasGenericThrowaway`: any of `acceptableThrowawayItems` within reach of the hotbar.
pub fn has_generic_throwaway(inv: &Inventory, settings: &Settings) -> bool {
    settings.acceptable_throwaway_items.iter().any(|item| {
        (0..9).any(|i| inv.get(i).is(item))
            || (inv.offhand.is(item) && (0..9).any(|i| inv.get(i).is_empty() || inv.get(i).is_tool))
            || (settings.allow_inventory && (9..36).any(|i| inv.get(i).is(item)))
    })
}

fn first_valid_throwaway(inv: &Inventory, settings: &Settings) -> Option<usize> {
    (0..inv.items.len()).find(|&i| {
        let s = inv.get(i);
        !s.is_empty() && settings.acceptable_throwaway_items.contains(&s.id)
    })
}

fn best_tool_against(inv: &Inventory, settings: &Settings, against: &crate::world::BlockInfo) -> Option<usize> {
    let mut best = None;
    let mut best_speed = -1.0;
    for i in 0..inv.items.len() {
        let stack = inv.get(i);
        if stack.is_empty() {
            continue;
        }
        if settings.item_saver
            && stack.damage + settings.item_saver_threshold >= stack.max_damage
            && stack.max_damage > 1
        {
            continue;
        }
        if stack.is_tool {
            let speed = ToolSet::calculate_speed_vs_block(stack, against);
            if speed > best_speed {
                best_speed = speed;
                best = Some(i);
            }
        }
    }
    best
}
