//! What Baritone sees of the game each tick ([`GameState`]) and what it asks the game to do ([`Controls`]): the
//! Rust side of `IPlayerContext`, `IPlayerController` and the input/look overrides. The game builds the state
//! before Baritone's tick and applies the controls after it.

use crate::math::{Aabb, BlockPos, Rotation, Vec3};
use crate::world::{BlockId, HitResult, WorldView};
use std::sync::Arc;

/// `GameType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum GameMode {
    #[default]
    Survival,
    Creative,
    Adventure,
    Spectator,
}

impl GameMode {
    pub fn is_creative(self) -> bool {
        self == GameMode::Creative
    }
}

/// How fast and how well one item mines each block: `ItemStack.getDestroySpeed(state)` and
/// `isCorrectToolForDrops(state)`, by block, as the game's own tool rules answer them.
#[derive(Clone, Debug, Default)]
pub struct ToolSpeeds {
    /// By [`BlockId`].
    pub speed: Vec<f32>,
    /// By [`BlockId`].
    pub correct_for_drops: Vec<bool>,
}

impl ToolSpeeds {
    pub fn speed(&self, block: BlockId) -> f32 {
        self.speed.get(block.0 as usize).copied().unwrap_or(1.0)
    }
    pub fn correct(&self, block: BlockId) -> bool {
        self.correct_for_drops
            .get(block.0 as usize)
            .copied()
            .unwrap_or(false)
    }
}

/// An item stack as Baritone looks at it.
#[derive(Clone, Debug, Default)]
pub struct ItemStack {
    /// The item's id, `minecraft:diamond_pickaxe`; empty for an empty slot.
    pub id: Arc<str>,
    pub count: i32,
    pub damage: i32,
    pub max_damage: i32,
    /// In `#minecraft:swords`.
    pub is_sword: bool,
    /// Has the `minecraft:tool` component (pickaxes, axes, shovels, hoes, shears, swords).
    pub is_tool: bool,
    /// `ToolSet.getMaterialCost`: the index of the first of the wooden, stone, iron, gold, diamond and
    /// netherite tool material tags the item is in, or -1.
    pub material_cost: i32,
    /// Has Silk Touch.
    pub silk_touch: bool,
    /// What its enchantments add to `minecraft:mining_efficiency` (Efficiency V: 26).
    pub mining_efficiency: f32,
    /// Frost Walker's level, on boots.
    pub frost_walker: i32,
    /// What its enchantments set `minecraft:water_movement_efficiency` to (Depth Strider), 0 if none.
    pub water_movement_efficiency: f32,
    /// The block a `BlockItem` places, by id.
    pub places_block: Option<Arc<str>>,
    /// Mining speeds, for tools and anything else that can hit a block.
    pub speeds: Option<Arc<ToolSpeeds>>,
    /// Food that can be eaten now.
    pub edible: bool,
}

impl ItemStack {
    pub fn is_empty(&self) -> bool {
        self.id.is_empty() || &*self.id == "minecraft:air" || self.count <= 0
    }
    pub fn is(&self, id: &str) -> bool {
        !self.is_empty() && &*self.id == id
    }
}

/// A player's inventory: `items[0..9]` is the hotbar, then the main inventory, as `Inventory.items`.
#[derive(Clone, Debug, Default)]
pub struct Inventory {
    pub items: Vec<ItemStack>,
    /// Feet, legs, chest, head.
    pub armor: Vec<ItemStack>,
    pub offhand: ItemStack,
    pub selected: usize,
    /// The stack held by the cursor in an open menu.
    pub carried: ItemStack,
}

impl Inventory {
    pub fn get(&self, slot: usize) -> &ItemStack {
        static EMPTY: std::sync::OnceLock<ItemStack> = std::sync::OnceLock::new();
        self.items
            .get(slot)
            .unwrap_or_else(|| EMPTY.get_or_init(ItemStack::default))
    }
    pub fn held(&self) -> &ItemStack {
        self.get(self.selected)
    }
    /// `Inventory.findSlotMatchingItem`: the first slot holding the item.
    pub fn find(&self, id: &str) -> Option<usize> {
        self.items.iter().position(|s| s.is(id))
    }
    /// The first hotbar slot holding the item.
    pub fn find_in_hotbar(&self, id: &str) -> Option<usize> {
        self.find(id).filter(|&s| s < 9)
    }
}

/// An entity the client knows about.
#[derive(Clone, Debug)]
pub struct EntityInfo {
    pub id: i32,
    /// `minecraft:zombie`.
    pub kind: Arc<str>,
    pub position: Vec3,
    pub bounding_box: Aabb,
    pub alive: bool,
    /// For players: their name.
    pub name: Option<Arc<str>>,
    pub uuid: u128,
    /// For dropped items: the item's id.
    pub item: Option<Arc<str>>,
    pub is_player: bool,
    /// Hostile to the player (`Enemy`).
    pub hostile: bool,
    /// A `Mob` (anything with AI, animals included).
    pub is_mob: bool,
    /// A zombified piglin someone hurt, or an enderman that is creepy (stared at).
    pub angry: bool,
    /// A falling block entity.
    pub falling_block: bool,
    /// Stops blocks being placed where it is (`blocksBuilding`: living things, minecarts, boats, TNT...).
    pub blocks_building: bool,
}

/// The local player.
#[derive(Clone, Debug, Default)]
pub struct PlayerState {
    /// Feet position.
    pub position: Vec3,
    /// `getDeltaMovement()`.
    pub velocity: Vec3,
    pub on_ground: bool,
    pub horizontal_collision: bool,
    pub yaw: f32,
    pub pitch: f32,
    /// The eye height of the current pose (1.62 standing, 1.27 crouching).
    pub eye_height: f64,
    /// `isCrouching()`: the crouching pose.
    pub crouching: bool,
    pub sprinting: bool,
    pub in_water: bool,
    pub in_lava: bool,
    /// `isInWall()`: suffocating inside a block.
    pub in_wall: bool,
    /// `getAbilities().flying`.
    pub flying: bool,
    pub may_fly: bool,
    /// Gliding with an elytra.
    pub fall_flying: bool,
    pub health: f32,
    pub food: i32,
    pub game_mode: GameMode,
    pub bounding_box: Aabb,
    /// The player's own entity id.
    pub entity_id: i32,
    pub name: String,
    /// Haste's amplifier, if it has Haste.
    pub haste: Option<i32>,
    /// Mining Fatigue's amplifier.
    pub mining_fatigue: Option<i32>,
    /// `isHandsBusy()`: rowing a boat.
    pub hands_busy: bool,
    /// Riding something.
    pub passenger: bool,
}

impl PlayerState {
    /// `playerFeet`: the block at the feet, nudged up for soul sand, and over a slab's top.
    pub fn feet_raw(&self) -> BlockPos {
        BlockPos::of(self.position.x, self.position.y + 0.1251, self.position.z)
    }
    pub fn head(&self) -> Vec3 {
        Vec3::new(
            self.position.x,
            self.position.y + self.eye_height,
            self.position.z,
        )
    }
    pub fn rotation(&self) -> Rotation {
        Rotation::new(self.yaw, self.pitch)
    }
    /// `RayTraceUtils.inferSneakingEyePosition`.
    pub fn sneaking_eyes(&self) -> Vec3 {
        Vec3::new(self.position.x, self.position.y + 1.27, self.position.z)
    }
    /// `getEyePosition(1)`.
    pub fn eyes(&self) -> Vec3 {
        self.head()
    }
}

/// What the game looks like this tick.
#[derive(Clone)]
pub struct GameState {
    /// None on the title screen and while a world loads.
    pub world: Option<WorldView>,
    pub player: Option<PlayerState>,
    pub inventory: Inventory,
    /// What the crosshair is on (`objectMouseOver`), as the game picked it.
    pub hit: HitResult,
    pub entities: Vec<EntityInfo>,
    /// A screen covers the world (an inventory, the pause menu...), the chat aside.
    pub screen_open: bool,
    /// A menu other than the player's own inventory is open (a chest, a furnace, a crafting table...).
    pub container_open: bool,
    /// The chat is open.
    pub chat_open: bool,
    /// Milliseconds since the epoch.
    pub now_ms: i64,
    /// The world's id for the files Baritone keeps (`baritone/<world>/...`), and the folder it goes in.
    pub world_dir: Option<std::path::PathBuf>,
    /// `minecraft:block_interaction_range` of the player.
    pub block_interaction_range: f64,
    /// Whether the player is the singleplayer host.
    pub singleplayer: bool,
    /// The mouse sensitivity option (0..1, 0.5 by default), which sets how finely the view can turn.
    pub mouse_sensitivity: f64,
    /// The local player's light level for `Spider` avoidance (`getLightLevelDependentMagicValue`).
    pub light_level: f32,
}

impl GameState {
    pub fn empty() -> Self {
        Self {
            world: None,
            player: None,
            inventory: Inventory::default(),
            hit: HitResult::Miss,
            entities: Vec::new(),
            screen_open: false,
            container_open: false,
            chat_open: false,
            now_ms: 0,
            world_dir: None,
            block_interaction_range: 4.5,
            singleplayer: false,
            mouse_sensitivity: 0.5,
            light_level: 1.0,
        }
    }
    pub fn in_world(&self) -> bool {
        self.world.is_some() && self.player.is_some()
    }
}

/// `ContainerInput` (`ClickType`): how an inventory slot is clicked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClickType {
    Pickup,
    QuickMove,
    /// Swap with hotbar slot `button`.
    Swap,
    Throw,
}

/// One click in the player's own inventory menu: `windowClick(inventoryMenu.containerId, slot, button, type)`.
/// `slot` is the menu slot (9..=35 main inventory, 36..=44 hotbar, 45 offhand).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InventoryClick {
    pub slot: i32,
    pub button: i32,
    pub kind: ClickType,
}

/// A piece of chat text Baritone prints.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Span {
    pub text: String,
    /// `ChatFormatting` colour as 0xRRGGBB.
    pub color: Option<u32>,
    pub bold: bool,
    pub italic: bool,
    pub underlined: bool,
    pub strikethrough: bool,
    /// Text shown when hovered.
    pub hover: Option<String>,
    pub click: Option<ClickEvent>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ClickEvent {
    RunCommand(String),
    SuggestCommand(String),
}

/// A line of chat: spans in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChatLine {
    pub spans: Vec<Span>,
}

impl ChatLine {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            spans: vec![Span {
                text: text.into(),
                ..Default::default()
            }],
        }
    }
    pub fn colored(text: impl Into<String>, color: u32) -> Self {
        Self {
            spans: vec![Span {
                text: text.into(),
                color: Some(color),
                ..Default::default()
            }],
        }
    }
    pub fn push(&mut self, span: Span) -> &mut Self {
        self.spans.push(span);
        self
    }
    pub fn text(&self) -> String {
        self.spans.iter().map(|s| s.text.as_str()).collect()
    }
}

/// `ChatFormatting` colours.
pub mod chat_color {
    pub const BLACK: u32 = 0x000000;
    pub const DARK_BLUE: u32 = 0x0000AA;
    pub const DARK_GREEN: u32 = 0x00AA00;
    pub const DARK_AQUA: u32 = 0x00AAAA;
    pub const DARK_RED: u32 = 0xAA0000;
    pub const DARK_PURPLE: u32 = 0xAA00AA;
    pub const GOLD: u32 = 0xFFAA00;
    pub const GRAY: u32 = 0xAAAAAA;
    pub const DARK_GRAY: u32 = 0x555555;
    pub const BLUE: u32 = 0x5555FF;
    pub const GREEN: u32 = 0x55FF55;
    pub const AQUA: u32 = 0x55FFFF;
    pub const RED: u32 = 0xFF5555;
    pub const LIGHT_PURPLE: u32 = 0xFF55FF;
    pub const YELLOW: u32 = 0xFFFF55;
    pub const WHITE: u32 = 0xFFFFFF;
}

/// A toast or desktop notification Baritone wants shown (`notifier`, `toaster`).
#[derive(Clone, Debug, PartialEq)]
pub struct Notification {
    pub title: String,
    pub message: String,
    pub error: bool,
}

/// The movement keys Baritone holds while it controls the player (`PlayerMovementInput`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MovementKeys {
    pub forward: bool,
    pub back: bool,
    pub left: bool,
    pub right: bool,
    pub jump: bool,
    pub sneak: bool,
    pub sprint: bool,
}

impl MovementKeys {
    /// `[strafe, forward]`, strafe positive to the left, as `ClientInput.moveVector`.
    pub fn impulse(&self) -> [f32; 2] {
        let mut forward = 0.0;
        let mut left = 0.0;
        if self.forward {
            forward += 1.0;
        }
        if self.back {
            forward -= 1.0;
        }
        if self.left {
            left += 1.0;
        }
        if self.right {
            left -= 1.0;
        }
        [left, forward]
    }
}

/// What Baritone asks the game to do after a tick.
#[derive(Clone, Debug, Default)]
pub struct Controls {
    /// Turn the player to this rotation now.
    pub rotation: Option<Rotation>,
    /// While `Some`, Baritone owns the movement keys (the player's own are ignored).
    pub movement: Option<MovementKeys>,
    /// Hold the attack key (mining the block under the crosshair).
    pub attack: bool,
    /// Click the use key once this tick (placing against the block under the crosshair).
    pub use_item: bool,
    /// Select this hotbar slot.
    pub select_slot: Option<usize>,
    /// Inventory clicks to make, in order.
    pub inventory_clicks: Vec<InventoryClick>,
    /// Lines to print in the chat, client side only.
    pub chat: Vec<ChatLine>,
    /// Notifications to show.
    pub notifications: Vec<Notification>,
    /// Stop flying (creative flight) for the movement.
    pub stop_flying: bool,
    /// Messages to send to the server as the player.
    pub send_chat: Vec<String>,
}
