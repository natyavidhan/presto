//! What Baritone knows about each block state.
//!
//! The game describes every state once (`RawState`: its block's id, Java class chain, properties, shapes and
//! a few vanilla facts), and [`BlockInfo::classify`] turns that into the flags Baritone's movement code asks
//! about (`block instanceof DoorBlock`, `state.getValue(SlabBlock.TYPE)`, `Block.isShapeFullBlock(...)`).
//! The table is built when a world opens and is shared, read-only, with the path-finding threads.

use crate::math::{Aabb, BlockPos, Direction};
use rustc_hash::FxHashMap;
use std::sync::Arc;

/// A block state's id in the game's registry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct StateId(pub u32);

/// A block's index in [`BlockTable::blocks`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct BlockId(pub u32);

/// The fluid a state holds (`FluidState.getType()`, flowing and still alike).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Fluid {
    #[default]
    None,
    Water,
    Lava,
    /// A fluid a mod adds: neither water nor lava to Baritone.
    Other,
}

/// `SlabBlock.TYPE`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum SlabType {
    #[default]
    NotASlab,
    Bottom,
    Top,
    Double,
}

/// Flags of [`BlockInfo::flags`]: the block classes and specific blocks Baritone tests for.
pub mod flags {
    pub const AIR: u64 = 1 << 0;
    /// `BaseFireBlock`.
    pub const FIRE: u64 = 1 << 1;
    pub const COBWEB: u64 = 1 << 2;
    /// `Blocks.END_PORTAL` / `EndPortalBlock`.
    pub const END_PORTAL: u64 = 1 << 3;
    pub const COCOA: u64 = 1 << 4;
    /// `AbstractSkullBlock`: heads and skulls, on the floor or a wall.
    pub const ABSTRACT_SKULL: u64 = 1 << 5;
    /// `SkullBlock`: a floor head or skull.
    pub const SKULL: u64 = 1 << 6;
    pub const BUBBLE_COLUMN: u64 = 1 << 7;
    pub const SHULKER_BOX: u64 = 1 << 8;
    pub const SLAB: u64 = 1 << 9;
    pub const TRAPDOOR: u64 = 1 << 10;
    pub const HONEY_BLOCK: u64 = 1 << 11;
    pub const END_ROD: u64 = 1 << 12;
    pub const SWEET_BERRY_BUSH: u64 = 1 << 13;
    pub const POINTED_DRIPSTONE: u64 = 1 << 14;
    /// `AmethystClusterBlock`: the clusters and the buds.
    pub const AMETHYST_CLUSTER: u64 = 1 << 15;
    /// `AzaleaBlock`: azalea and flowering azalea.
    pub const AZALEA: u64 = 1 << 16;
    pub const BIG_DRIPLEAF: u64 = 1 << 17;
    pub const POWDER_SNOW: u64 = 1 << 18;
    pub const DOOR: u64 = 1 << 19;
    pub const IRON_DOOR: u64 = 1 << 20;
    pub const FENCE_GATE: u64 = 1 << 21;
    /// `CarpetBlock`.
    pub const CARPET: u64 = 1 << 22;
    /// `SnowLayerBlock`.
    pub const SNOW_LAYER: u64 = 1 << 23;
    /// `CauldronBlock`: the empty cauldron.
    pub const CAULDRON: u64 = 1 << 24;
    pub const TRIPWIRE: u64 = 1 << 25;
    pub const VINE: u64 = 1 << 26;
    pub const LADDER: u64 = 1 << 27;
    pub const LILY_PAD: u64 = 1 << 28;
    /// `LiquidBlock`: the water and lava blocks themselves.
    pub const LIQUID_BLOCK: u64 = 1 << 29;
    /// `FallingBlock`: sand, gravel, concrete powder, anvils, the dragon egg...
    pub const FALLING_BLOCK: u64 = 1 << 30;
    /// `InfestedBlock`.
    pub const INFESTED: u64 = 1 << 31;
    /// `Blocks.ICE` itself (it melts into water).
    pub const ICE: u64 = 1 << 32;
    pub const MAGMA: u64 = 1 << 33;
    pub const CACTUS: u64 = 1 << 34;
    /// `StainedGlassBlock`.
    pub const STAINED_GLASS: u64 = 1 << 35;
    /// `Blocks.GLASS` itself.
    pub const GLASS: u64 = 1 << 36;
    /// `StairBlock`.
    pub const STAIRS: u64 = 1 << 37;
    /// Chest, trapped chest and ender chest.
    pub const CHEST: u64 = 1 << 38;
    pub const FARMLAND: u64 = 1 << 39;
    pub const DIRT_PATH: u64 = 1 << 40;
    pub const SOUL_SAND: u64 = 1 << 41;
    /// `LeavesBlock`.
    pub const LEAVES: u64 = 1 << 42;
    pub const SCAFFOLDING: u64 = 1 << 43;
    /// `BambooStalkBlock`.
    pub const BAMBOO_STALK: u64 = 1 << 44;
    /// `MovingPistonBlock`.
    pub const MOVING_PISTON: u64 = 1 << 45;
    /// `MovementHelper.isClimbable`: ladders, vines, weeping and twisting vines.
    pub const CLIMBABLE: u64 = 1 << 46;
    /// Tall grass and large fern (always replaceable to Baritone).
    pub const TALL_GRASS: u64 = 1 << 47;
    /// `Block.isShapeFullBlock(state.getCollisionShape(..))`.
    pub const FULL_COLLISION: u64 = 1 << 48;
    /// `state.isPathfindable(PathComputationType.LAND)`.
    pub const PATHFINDABLE_LAND: u64 = 1 << 49;
    /// `state.canBeReplaced()`.
    pub const REPLACEABLE: u64 = 1 << 50;
    /// `BedBlock`.
    pub const BED: u64 = 1 << 51;
    /// `FenceBlock`, `WallBlock`: taller than a block to step over.
    pub const FENCE_LIKE: u64 = 1 << 52;
    /// The state's own `waterlogged` property is true.
    pub const WATERLOGGED: u64 = 1 << 53;
    /// `EndPortalBlock` (the class, which the end gateway is not).
    pub const END_PORTAL_CLASS: u64 = 1 << 54;
    /// `Blocks.WATER.defaultBlockState()`: a still water source block, what frosted ice melts into.
    pub const WATER_SOURCE_BLOCK: u64 = 1 << 55;
    /// `LilyPadBlock` / carpets above water: something to stand on that floats.
    pub const FLOATING: u64 = 1 << 56;
    /// `Blocks.OBSIDIAN` and the like that Baritone never mines by accident (`blocksToAvoidBreaking` aside).
    pub const NETHER_PORTAL: u64 = 1 << 57;
    /// A container block (chest, barrel, furnace...): it opens on a right click.
    pub const INTERACTABLE: u64 = 1 << 58;
    /// `CropBlock`-like blocks with an `age` property that grow (FarmProcess).
    pub const CROP: u64 = 1 << 59;
    /// `FrostedIceBlock`.
    pub const FROSTED_ICE: u64 = 1 << 60;
    /// `TallGrassBlock`, `DoublePlantBlock` or `FlowerBlock`: cached as air by `ChunkPacker`.
    pub const PLANT: u64 = 1 << 61;
    /// `BlockState.blocksMotion()` (legacy solidity, cobwebs and bamboo saplings excepted).
    pub const BLOCKS_MOTION: u64 = 1 << 62;
}

/// A state as the game describes it to Baritone: the raw material of [`BlockInfo::classify`].
#[derive(Clone, Debug, Default)]
pub struct RawState {
    /// The block's id, `minecraft:oak_stairs`.
    pub name: String,
    /// The block's Java class chain, most specific first, simple names (`StairBlock`, `Block`).
    pub classes: Vec<String>,
    /// The state's properties, `("half", "top")`.
    pub properties: Vec<(String, String)>,
    /// `getDestroySpeed`; negative for unbreakable blocks.
    pub hardness: f32,
    pub requires_tool: bool,
    pub replaceable: bool,
    pub fluid: Fluid,
    /// `FluidState.getAmount()`: 0 with no fluid, 8 for a source or a falling column.
    pub fluid_amount: u8,
    /// `FluidState.isSource()`.
    pub fluid_source: bool,
    /// The collision shape with an empty context, block-local.
    pub collision: Vec<Aabb>,
    /// The outline (`getShape`), block-local, what the crosshair picks.
    pub outline: Vec<Aabb>,
    /// `isPathfindable(LAND)`, when the game knows it; otherwise it is worked out as vanilla's default does.
    pub pathfindable_land: Option<bool>,
    /// `isCollisionShapeFullBlock`, when the game knows it.
    pub full_collision: Option<bool>,
    /// `BlockState.isSolid()`, vanilla's legacy solidity, when the game knows it.
    pub legacy_solid: Option<bool>,
    /// The block's item (`minecraft:air` for none).
    pub item: String,
}

/// Everything Baritone asks about one block state.
///
/// `repr(C)` with the fields the search reads first: they share the struct's first cache line, and the shapes,
/// names and properties (read rarely) come after.
#[derive(Clone, Debug)]
#[repr(C)]
pub struct BlockInfo {
    pub flags: u64,
    pub block: BlockId,
    pub hardness: f32,
    pub fluid: Fluid,
    pub fluid_amount: u8,
    pub fluid_source: bool,
    /// `LiquidBlock.LEVEL` for the water and lava blocks (0 for a source).
    pub liquid_level: u8,
    pub slab: SlabType,
    /// `half=top` (stairs, trapdoors) or `half=upper` (doors, tall plants).
    pub half_top: bool,
    /// `StairBlock.SHAPE` is `inner_left` or `inner_right`.
    pub stair_inner: bool,
    /// The `open` property.
    pub open: bool,
    /// The `facing` property, when horizontal or vertical.
    pub facing: Option<Direction>,
    /// `SnowLayerBlock.LAYERS`.
    pub snow_layers: u8,
    pub requires_tool: bool,
    pub name: Arc<str>,
    pub collision: Arc<[Aabb]>,
    pub outline: Arc<[Aabb]>,
    pub properties: Arc<[(Arc<str>, Arc<str>)]>,
    pub item: Arc<str>,
}

impl BlockInfo {
    #[inline]
    pub fn is(&self, flag: u64) -> bool {
        self.flags & flag != 0
    }
    #[inline]
    pub fn is_air(&self) -> bool {
        self.is(flags::AIR)
    }
    /// `!state.getFluidState().isEmpty()`.
    #[inline]
    pub fn has_fluid(&self) -> bool {
        self.fluid != Fluid::None
    }
    pub fn is_water(&self) -> bool {
        self.fluid == Fluid::Water
    }
    pub fn is_lava(&self) -> bool {
        self.fluid == Fluid::Lava
    }
    pub fn property(&self, name: &str) -> Option<&str> {
        self.properties
            .iter()
            .find(|(k, _)| &**k == name)
            .map(|(_, v)| &**v)
    }
    /// The `age` of a crop, and the age it is ripe at.
    pub fn age(&self) -> Option<u8> {
        self.property("age").and_then(|a| a.parse().ok())
    }
    /// The highest point of the collision shape (0 for none).
    pub fn collision_max_y(&self) -> f64 {
        self.collision.iter().map(|b| b.max.y).fold(0.0, f64::max)
    }
    /// The collision shape's extent along an axis, or `None` if it is empty.
    pub fn collision_bounds(&self) -> Option<Aabb> {
        bounds(&self.collision)
    }
    pub fn outline_bounds(&self) -> Option<Aabb> {
        bounds(&self.outline)
    }

    /// Builds what Baritone knows about a state from the game's description of it.
    pub fn classify(raw: &RawState, block: BlockId) -> BlockInfo {
        let name = raw.name.as_str();
        let path = name.strip_prefix("minecraft:").unwrap_or(name);
        let vanilla = name.starts_with("minecraft:");
        let has_class = |c: &str| raw.classes.iter().any(|x| x == c);
        let prop = |k: &str| {
            raw.properties
                .iter()
                .find(|(n, _)| n == k)
                .map(|(_, v)| v.as_str())
        };
        let mut f = 0u64;
        macro_rules! set {
            ($cond:expr, $flag:expr $(,)?) => {
                if $cond {
                    f |= $flag;
                }
            };
        }
        let is = |p: &str| vanilla && path == p;

        set!(
            has_class("AirBlock") || is("air") || is("cave_air") || is("void_air"),
            flags::AIR,
        );
        set!(
            has_class("BaseFireBlock") || is("fire") || is("soul_fire"),
            flags::FIRE,
        );
        set!(is("cobweb"), flags::COBWEB);
        set!(is("end_portal"), flags::END_PORTAL);
        set!(
            has_class("EndPortalBlock") || is("end_portal"),
            flags::END_PORTAL_CLASS,
        );
        set!(is("cocoa"), flags::COCOA);
        let skull_like = has_class("AbstractSkullBlock")
            || (vanilla && (path.ends_with("_skull") || path.ends_with("_head")));
        set!(skull_like, flags::ABSTRACT_SKULL);
        set!(
            has_class("SkullBlock")
                || (skull_like && !path.contains("wall_") && !has_class("WallSkullBlock")),
            flags::SKULL,
        );
        set!(
            is("bubble_column") || has_class("BubbleColumnBlock"),
            flags::BUBBLE_COLUMN,
        );
        set!(
            has_class("ShulkerBoxBlock") || (vanilla && path.ends_with("shulker_box")),
            flags::SHULKER_BOX,
        );
        set!(
            has_class("SlabBlock") || (vanilla && path.ends_with("_slab")),
            flags::SLAB,
        );
        set!(
            has_class("TrapDoorBlock") || (vanilla && path.ends_with("_trapdoor")),
            flags::TRAPDOOR,
        );
        set!(is("honey_block"), flags::HONEY_BLOCK);
        set!(is("end_rod"), flags::END_ROD);
        set!(is("sweet_berry_bush"), flags::SWEET_BERRY_BUSH);
        set!(
            is("pointed_dripstone") || has_class("PointedDripstoneBlock"),
            flags::POINTED_DRIPSTONE,
        );
        set!(
            has_class("AmethystClusterBlock")
                || is("amethyst_cluster")
                || (vanilla && path.ends_with("_amethyst_bud")),
            flags::AMETHYST_CLUSTER,
        );
        set!(
            has_class("AzaleaBlock") || is("azalea") || is("flowering_azalea"),
            flags::AZALEA,
        );
        set!(is("big_dripleaf"), flags::BIG_DRIPLEAF);
        set!(is("powder_snow"), flags::POWDER_SNOW);
        set!(
            has_class("DoorBlock") || (vanilla && path.ends_with("_door")),
            flags::DOOR,
        );
        set!(is("iron_door"), flags::IRON_DOOR);
        set!(
            has_class("FenceGateBlock") || (vanilla && path.ends_with("_fence_gate")),
            flags::FENCE_GATE,
        );
        set!(
            has_class("CarpetBlock")
                || (vanilla && path.ends_with("_carpet") && path != "pale_moss_carpet"),
            flags::CARPET,
        );
        set!(
            has_class("SnowLayerBlock") || is("snow"),
            flags::SNOW_LAYER,
        );
        set!(
            (has_class("CauldronBlock") && !has_class("LayeredCauldronBlock"))
                || is("cauldron"),
            flags::CAULDRON,
        );
        set!(is("tripwire"), flags::TRIPWIRE);
        set!(is("vine"), flags::VINE);
        set!(is("ladder"), flags::LADDER);
        set!(is("lily_pad"), flags::LILY_PAD);
        set!(
            has_class("LiquidBlock") || is("water") || is("lava"),
            flags::LIQUID_BLOCK,
        );
        set!(
            has_class("FallingBlock")
                || is("sand")
                || is("red_sand")
                || is("gravel")
                || is("anvil")
                || is("chipped_anvil")
                || is("damaged_anvil")
                || is("dragon_egg")
                || (vanilla && path.ends_with("_concrete_powder")),
            flags::FALLING_BLOCK,
        );
        set!(
            has_class("InfestedBlock") || has_class("InfestedRotatedPillarBlock") || (vanilla && path.starts_with("infested_")),
            flags::INFESTED,
        );
        set!(is("ice"), flags::ICE);
        set!(is("frosted_ice"), flags::FROSTED_ICE);
        set!(is("magma_block"), flags::MAGMA);
        set!(is("cactus"), flags::CACTUS);
        set!(
            has_class("StainedGlassBlock") || (vanilla && path.ends_with("_stained_glass")),
            flags::STAINED_GLASS,
        );
        set!(is("glass"), flags::GLASS);
        set!(
            has_class("StairBlock") || (vanilla && path.ends_with("_stairs")),
            flags::STAIRS,
        );
        set!(
            is("chest") || is("trapped_chest") || is("ender_chest"),
            flags::CHEST,
        );
        set!(is("farmland"), flags::FARMLAND);
        set!(is("dirt_path"), flags::DIRT_PATH);
        set!(is("soul_sand"), flags::SOUL_SAND);
        set!(
            has_class("LeavesBlock") || (vanilla && path.ends_with("_leaves")),
            flags::LEAVES,
        );
        set!(is("scaffolding"), flags::SCAFFOLDING);
        set!(
            has_class("BambooStalkBlock") || is("bamboo"),
            flags::BAMBOO_STALK,
        );
        set!(
            has_class("MovingPistonBlock") || is("moving_piston"),
            flags::MOVING_PISTON,
        );
        set!(
            is("ladder")
                || is("vine")
                || is("weeping_vines")
                || is("weeping_vines_plant")
                || is("twisting_vines")
                || is("twisting_vines_plant"),
            flags::CLIMBABLE,
        );
        set!(is("tall_grass") || is("large_fern"), flags::TALL_GRASS);
        set!(
            has_class("BedBlock") || (vanilla && path.ends_with("_bed")),
            flags::BED,
        );
        set!(
            has_class("FenceBlock")
                || has_class("WallBlock")
                || (vanilla && (path.ends_with("_fence") || path.ends_with("_wall")) && !path.contains("sign") && !path.contains("banner") && !path.contains("torch") && !path.contains("head") && !path.contains("skull") && !path.contains("fan")),
            flags::FENCE_LIKE,
        );
        set!(prop("waterlogged") == Some("true"), flags::WATERLOGGED);
        set!(is("nether_portal"), flags::NETHER_PORTAL);
        set!(
            raw.classes.iter().any(|c| {
                matches!(
                    c.as_str(),
                    "AbstractChestBlock"
                        | "BarrelBlock"
                        | "AbstractFurnaceBlock"
                        | "CraftingTableBlock"
                        | "EnchantingTableBlock"
                        | "AnvilBlock"
                        | "BeaconBlock"
                        | "BrewingStandBlock"
                        | "DispenserBlock"
                        | "HopperBlock"
                        | "ShulkerBoxBlock"
                        | "LoomBlock"
                        | "CartographyTableBlock"
                        | "GrindstoneBlock"
                        | "StonecutterBlock"
                        | "SmithingTableBlock"
                        | "LecternBlock"
                        | "ButtonBlock"
                        | "LeverBlock"
                        | "NoteBlock"
                        | "RepeaterBlock"
                        | "ComparatorBlock"
                        | "BellBlock"
                        | "CrafterBlock"
                        | "DoorBlock"
                        | "TrapDoorBlock"
                        | "FenceGateBlock"
                        | "BedBlock"
                        | "RespawnAnchorBlock"
                        | "ChiseledBookShelfBlock"
                        | "DecoratedPotBlock"
                        | "CakeBlock"
                        | "CandleCakeBlock"
                        | "FlowerPotBlock"
                        | "SignBlock"
                        | "CeilingHangingSignBlock"
                        | "WallHangingSignBlock"
                        | "JukeboxBlock"
                        | "ComposterBlock"
                        | "CauldronBlock"
                        | "LayeredCauldronBlock"
                        | "LavaCauldronBlock"
                        | "DaylightDetectorBlock"
                        | "CommandBlock"
                        | "StructureBlock"
                        | "JigsawBlock"
                        | "VaultBlock"
                        | "TrialSpawnerBlock"
                )
            }) || (vanilla
                && matches!(
                    path,
                    "chest"
                        | "trapped_chest"
                        | "ender_chest"
                        | "barrel"
                        | "furnace"
                        | "blast_furnace"
                        | "smoker"
                        | "crafting_table"
                        | "crafter"
                        | "enchanting_table"
                        | "anvil"
                        | "chipped_anvil"
                        | "damaged_anvil"
                        | "beacon"
                        | "brewing_stand"
                        | "dispenser"
                        | "dropper"
                        | "hopper"
                        | "loom"
                        | "cartography_table"
                        | "grindstone"
                        | "stonecutter"
                        | "smithing_table"
                        | "lectern"
                        | "lever"
                        | "note_block"
                        | "repeater"
                        | "comparator"
                        | "bell"
                        | "respawn_anchor"
                        | "cake"
                        | "jukebox"
                        | "composter"
                        | "daylight_detector"
                        | "flower_pot"
                )),
            flags::INTERACTABLE,
        );
        set!(
            vanilla
                && matches!(
                    path,
                    "wheat"
                        | "carrots"
                        | "potatoes"
                        | "beetroots"
                        | "nether_wart"
                        | "cocoa"
                        | "melon_stem"
                        | "pumpkin_stem"
                        | "torchflower_crop"
                        | "pitcher_crop"
                        | "sweet_berry_bush"
                ),
            flags::CROP,
        );

        let fluid = raw.fluid;
        let liquid_level = if f & flags::LIQUID_BLOCK != 0 {
            prop("level").and_then(|v| v.parse().ok()).unwrap_or(0)
        } else {
            0
        };
        set!(
            vanilla && path == "water" && liquid_level == 0,
            flags::WATER_SOURCE_BLOCK,
        );
        set!(
            f & (flags::LILY_PAD | flags::CARPET) != 0,
            flags::FLOATING,
        );

        set!(
            raw.classes.iter().any(|c| {
                matches!(
                    c.as_str(),
                    "TallGrassBlock" | "DoublePlantBlock" | "FlowerBlock"
                )
            }) || (vanilla
                && matches!(
                    path,
                    "short_grass"
                        | "fern"
                        | "tall_grass"
                        | "large_fern"
                        | "sunflower"
                        | "lilac"
                        | "rose_bush"
                        | "peony"
                        | "dandelion"
                        | "poppy"
                        | "blue_orchid"
                        | "allium"
                        | "azure_bluet"
                        | "red_tulip"
                        | "orange_tulip"
                        | "white_tulip"
                        | "pink_tulip"
                        | "oxeye_daisy"
                        | "cornflower"
                        | "lily_of_the_valley"
                        | "wither_rose"
                        | "torchflower"
                        | "open_eyeblossom"
                        | "closed_eyeblossom"
                )),
            flags::PLANT,
        );
        let full = raw
            .full_collision
            .unwrap_or_else(|| is_full_block(&raw.collision));
        set!(full, flags::FULL_COLLISION);
        // `BlockState.blocksMotion`: solid, and neither a cobweb nor a bamboo sapling.
        let legacy_solid = raw.legacy_solid.unwrap_or(!raw.collision.is_empty());
        set!(
            legacy_solid && !is("cobweb") && !is("bamboo_sapling"),
            flags::BLOCKS_MOTION,
        );
        // `BlockBehaviour.isPathfindable` by default: LAND when the collision shape isn't a full block.
        set!(raw.pathfindable_land.unwrap_or(!full), flags::PATHFINDABLE_LAND);
        set!(
            raw.replaceable || f & flags::AIR != 0,
            flags::REPLACEABLE,
        );

        let slab = if f & flags::SLAB != 0 {
            match prop("type") {
                Some("top") => SlabType::Top,
                Some("double") => SlabType::Double,
                _ => SlabType::Bottom,
            }
        } else {
            SlabType::NotASlab
        };
        let half_top = matches!(prop("half"), Some("top") | Some("upper"));
        let stair_inner = matches!(prop("shape"), Some("inner_left") | Some("inner_right"));
        let open = prop("open") == Some("true");
        let facing = prop("facing").and_then(Direction::from_name);
        let snow_layers = prop("layers").and_then(|v| v.parse().ok()).unwrap_or(0);

        let collision: Arc<[Aabb]> = raw.collision.clone().into();
        let outline: Arc<[Aabb]> = if raw.outline.is_empty() && !raw.collision.is_empty() {
            collision.clone()
        } else {
            raw.outline.clone().into()
        };
        BlockInfo {
            name: Arc::from(raw.name.as_str()),
            block,
            flags: f,
            fluid,
            fluid_amount: raw.fluid_amount,
            fluid_source: raw.fluid_source,
            liquid_level,
            slab,
            half_top,
            stair_inner,
            open,
            facing,
            snow_layers,
            hardness: raw.hardness,
            requires_tool: raw.requires_tool,
            collision,
            outline,
            properties: raw
                .properties
                .iter()
                .map(|(k, v)| (Arc::from(k.as_str()), Arc::from(v.as_str())))
                .collect(),
            item: Arc::from(raw.item.as_str()),
        }
    }
}

fn bounds(boxes: &[Aabb]) -> Option<Aabb> {
    let first = *boxes.first()?;
    Some(boxes.iter().skip(1).fold(first, |a, b| {
        Aabb::new(
            a.min.x.min(b.min.x),
            a.min.y.min(b.min.y),
            a.min.z.min(b.min.z),
            a.max.x.max(b.max.x),
            a.max.y.max(b.max.y),
            a.max.z.max(b.max.z),
        )
    }))
}

/// `Block.isShapeFullBlock`: whether the boxes fill the unit cube exactly.
pub fn is_full_block(boxes: &[Aabb]) -> bool {
    if boxes.is_empty() {
        return false;
    }
    let eps = 1.0e-7;
    // Nothing may stick out of the cube...
    if boxes.iter().any(|b| {
        b.min.x < -eps
            || b.min.y < -eps
            || b.min.z < -eps
            || b.max.x > 1.0 + eps
            || b.max.y > 1.0 + eps
            || b.max.z > 1.0 + eps
    }) {
        return false;
    }
    // ...and every cell of the grid the boxes' faces make must be covered.
    let mut xs: Vec<f64> = vec![0.0, 1.0];
    let mut ys = xs.clone();
    let mut zs = xs.clone();
    for b in boxes {
        xs.extend([b.min.x, b.max.x]);
        ys.extend([b.min.y, b.max.y]);
        zs.extend([b.min.z, b.max.z]);
    }
    for v in [&mut xs, &mut ys, &mut zs] {
        v.retain(|c| (-eps..=1.0 + eps).contains(c));
        v.sort_by(|a, b| a.total_cmp(b));
        v.dedup_by(|a, b| (*a - *b).abs() < eps);
    }
    for xw in xs.windows(2) {
        for yw in ys.windows(2) {
            for zw in zs.windows(2) {
                let c = crate::math::Vec3::new(
                    (xw[0] + xw[1]) / 2.0,
                    (yw[0] + yw[1]) / 2.0,
                    (zw[0] + zw[1]) / 2.0,
                );
                if !boxes.iter().any(|b| b.contains(c)) {
                    return false;
                }
            }
        }
    }
    true
}

/// One block (not state): its id and the range of its states.
#[derive(Clone, Debug)]
pub struct BlockDef {
    pub name: Arc<str>,
    pub default_state: StateId,
    pub states: Vec<StateId>,
}

/// Every state the game has, classified.
#[derive(Clone, Debug)]
pub struct BlockTable {
    states: Vec<BlockInfo>,
    blocks: Vec<BlockDef>,
    by_name: FxHashMap<Arc<str>, BlockId>,
    air: StateId,
    /// A representative state for each cached-world kind.
    water: StateId,
    lava: StateId,
    /// The block a cached `SOLID` cell reads as when nothing better is known (`CachedChunk` uses obsidian).
    solid: StateId,
    /// What breaking each block drops, by [`BlockId`] (the block's own item unless the game says otherwise).
    drops: Vec<Arc<[Arc<str>]>>,
    /// Bumped each time a table is built, so caches keyed by state know when to start over.
    pub generation: u64,
}

impl BlockTable {
    /// Builds the table from the game's description of every state, in id order. `defaults` are the default
    /// states of the blocks (by id); a block without one takes its first state.
    pub fn build(raw: &[RawState], defaults: &FxHashMap<String, StateId>) -> BlockTable {
        static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let mut by_name: FxHashMap<Arc<str>, BlockId> = FxHashMap::default();
        let mut blocks: Vec<BlockDef> = Vec::new();
        let mut states = Vec::with_capacity(raw.len());
        for (i, r) in raw.iter().enumerate() {
            let id = match by_name.get(r.name.as_str()) {
                Some(id) => *id,
                None => {
                    let id = BlockId(blocks.len() as u32);
                    let name: Arc<str> = Arc::from(r.name.as_str());
                    blocks.push(BlockDef {
                        name: name.clone(),
                        default_state: defaults
                            .get(&r.name)
                            .copied()
                            .unwrap_or(StateId(i as u32)),
                        states: Vec::new(),
                    });
                    by_name.insert(name, id);
                    id
                }
            };
            blocks[id.0 as usize].states.push(StateId(i as u32));
            let mut info = BlockInfo::classify(r, id);
            // Share the name allocation with the block.
            info.name = blocks[id.0 as usize].name.clone();
            states.push(info);
        }
        let find = |name: &str| {
            by_name
                .get(name)
                .map(|b| blocks[b.0 as usize].default_state)
        };
        let air = find("minecraft:air").unwrap_or(StateId(0));
        let water = find("minecraft:water").unwrap_or(air);
        let lava = find("minecraft:lava").unwrap_or(air);
        let solid = find("minecraft:obsidian")
            .or_else(|| find("minecraft:stone"))
            .unwrap_or(air);
        let drops = blocks
            .iter()
            .map(|b| {
                let item = &states[b.default_state.0 as usize].item;
                if item.is_empty() || &**item == "minecraft:air" {
                    Arc::from(Vec::<Arc<str>>::new())
                } else {
                    Arc::from(vec![item.clone()])
                }
            })
            .collect();
        BlockTable {
            states,
            blocks,
            by_name,
            air,
            water,
            lava,
            solid,
            drops,
            generation: GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        }
    }

    #[inline]
    pub fn get(&self, id: StateId) -> &BlockInfo {
        self.states
            .get(id.0 as usize)
            .unwrap_or(&self.states[self.air.0 as usize])
    }
    pub fn len(&self) -> usize {
        self.states.len()
    }
    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
    }
    pub fn air(&self) -> StateId {
        self.air
    }
    pub fn water(&self) -> StateId {
        self.water
    }
    pub fn lava(&self) -> StateId {
        self.lava
    }
    pub fn solid(&self) -> StateId {
        self.solid
    }
    pub fn block_id(&self, name: &str) -> Option<BlockId> {
        if let Some(id) = self.by_name.get(name) {
            return Some(*id);
        }
        if !name.contains(':') {
            return self.by_name.get(format!("minecraft:{name}").as_str()).copied();
        }
        None
    }
    pub fn block(&self, id: BlockId) -> &BlockDef {
        &self.blocks[id.0 as usize]
    }
    pub fn blocks(&self) -> &[BlockDef] {
        &self.blocks
    }
    pub fn default_state(&self, name: &str) -> Option<StateId> {
        self.block_id(name)
            .map(|b| self.blocks[b.0 as usize].default_state)
    }
    pub fn states(&self) -> impl Iterator<Item = (StateId, &BlockInfo)> {
        self.states
            .iter()
            .enumerate()
            .map(|(i, s)| (StateId(i as u32), s))
    }
    /// The state of `block` with the given properties set, others as in its default state.
    pub fn with_properties(&self, block: BlockId, props: &[(&str, &str)]) -> Option<StateId> {
        let def = &self.blocks[block.0 as usize];
        let base = self.get(def.default_state);
        def.states.iter().copied().find(|s| {
            let info = self.get(*s);
            info.properties.iter().all(|(k, v)| {
                match props.iter().find(|(pk, _)| *pk == &**k) {
                    Some((_, pv)) => *pv == &**v,
                    None => base.property(k) == Some(&**v),
                }
            })
        })
    }
    /// The state like `state` but with one property changed.
    pub fn with_property(&self, state: StateId, key: &str, value: &str) -> Option<StateId> {
        let info = self.get(state);
        let def = &self.blocks[info.block.0 as usize];
        def.states.iter().copied().find(|s| {
            let other = self.get(*s);
            other.properties.iter().all(|(k, v)| {
                if &**k == key {
                    &**v == value
                } else {
                    info.property(k) == Some(&**v)
                }
            })
        })
    }
    /// What breaking a block drops (`BlockOptionalMeta.drops`).
    pub fn drops(&self, block: BlockId) -> &[Arc<str>] {
        self.drops.get(block.0 as usize).map_or(&[], |d| d)
    }

    /// Sets what breaking a block drops, from the game's loot tables.
    pub fn set_drops(&mut self, block: BlockId, items: Vec<Arc<str>>) {
        if let Some(d) = self.drops.get_mut(block.0 as usize) {
            *d = Arc::from(items);
        }
    }

    pub fn name_of(&self, state: StateId) -> &str {
        &self.get(state).name
    }
}

/// Where a ray stopped in the world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HitResult {
    Miss,
    Block {
        pos: BlockPos,
        face: Direction,
        location: crate::math::Vec3,
        /// The ray started inside the block.
        inside: bool,
    },
    Entity {
        id: i32,
        location: crate::math::Vec3,
    },
}

impl HitResult {
    pub fn block_pos(&self) -> Option<BlockPos> {
        match self {
            HitResult::Block { pos, .. } => Some(*pos),
            _ => None,
        }
    }
    pub fn face(&self) -> Option<Direction> {
        match self {
            HitResult::Block { face, .. } => Some(*face),
            _ => None,
        }
    }
    pub fn is_block(&self) -> bool {
        matches!(self, HitResult::Block { .. })
    }
}

/// A small vanilla-like block table for tests: the blocks Baritone's movement code singles out, with their
/// classes, properties and shapes as 26.3 has them.
#[cfg(test)]
pub mod testing {
    use super::*;

    fn cube() -> Vec<Aabb> {
        vec![Aabb::new(0.0, 0.0, 0.0, 1.0, 1.0, 1.0)]
    }

    pub struct TableBuilder {
        raw: Vec<RawState>,
    }

    impl TableBuilder {
        pub fn new() -> Self {
            Self { raw: Vec::new() }
        }
        pub fn add(&mut self, r: RawState) -> &mut Self {
            self.raw.push(r);
            self
        }
        pub fn simple(&mut self, name: &str, classes: &[&str], collision: Vec<Aabb>, hardness: f32) -> &mut Self {
            self.add(RawState {
                name: name.into(),
                classes: classes.iter().map(|s| s.to_string()).collect(),
                hardness,
                collision,
                item: name.into(),
                ..Default::default()
            })
        }
        pub fn build(&self) -> BlockTable {
            BlockTable::build(&self.raw, &FxHashMap::default())
        }
    }

    /// The blocks most tests need.
    pub fn vanilla() -> BlockTable {
        let mut t = TableBuilder::new();
        t.add(RawState {
            name: "minecraft:air".into(),
            classes: vec!["AirBlock".into()],
            replaceable: true,
            item: "minecraft:air".into(),
            ..Default::default()
        });
        t.simple("minecraft:stone", &["Block"], cube(), 1.5);
        t.simple("minecraft:dirt", &["Block"], cube(), 0.5);
        t.simple("minecraft:cobblestone", &["Block"], cube(), 2.0);
        t.simple("minecraft:obsidian", &["Block"], cube(), 50.0);
        t.simple("minecraft:bedrock", &["Block"], cube(), -1.0);
        t.simple("minecraft:sand", &["ColoredFallingBlock", "FallingBlock"], cube(), 0.5);
        t.simple("minecraft:gravel", &["ColoredFallingBlock", "FallingBlock"], cube(), 0.6);
        t.simple("minecraft:glass", &["TransparentBlock"], cube(), 0.3);
        t.simple("minecraft:magma_block", &["MagmaBlock"], cube(), 0.5);
        t.simple("minecraft:oak_leaves", &["TintedParticleLeavesBlock", "LeavesBlock"], cube(), 0.2);
        t.simple(
            "minecraft:soul_sand",
            &["SoulSandBlock"],
            vec![Aabb::new(0.0, 0.0, 0.0, 1.0, 0.875, 1.0)],
            0.5,
        );
        t.simple(
            "minecraft:farmland",
            &["FarmBlock"],
            vec![Aabb::new(0.0, 0.0, 0.0, 1.0, 0.9375, 1.0)],
            0.6,
        );
        t.simple("minecraft:ladder", &["LadderBlock"], vec![Aabb::new(0.0, 0.0, 0.8125, 1.0, 1.0, 1.0)], 0.4);
        t.simple("minecraft:vine", &["VineBlock"], vec![], 0.2);
        t.simple("minecraft:cobweb", &["WebBlock"], vec![], 4.0);
        t.simple("minecraft:fire", &["FireBlock", "BaseFireBlock"], vec![], 0.0);
        t.simple("minecraft:short_grass", &["TallGrassBlock"], vec![], 0.0);
        t.simple("minecraft:torch", &["TorchBlock"], vec![], 0.0);
        t.simple("minecraft:cactus", &["CactusBlock"], vec![Aabb::new(0.0625, 0.0, 0.0625, 0.9375, 0.9375, 0.9375)], 0.4);
        t.simple("minecraft:oak_fence", &["FenceBlock"], vec![Aabb::new(0.375, 0.0, 0.375, 0.625, 1.5, 0.625)], 2.0);
        t.simple("minecraft:white_carpet", &["WoolCarpetBlock", "CarpetBlock"], vec![Aabb::new(0.0, 0.0, 0.0, 1.0, 0.0625, 1.0)], 0.1);
        t.simple("minecraft:crafting_table", &["CraftingTableBlock"], cube(), 2.5);
        t.simple("minecraft:chest", &["ChestBlock", "AbstractChestBlock"], vec![Aabb::new(0.0625, 0.0, 0.0625, 0.9375, 0.875, 0.9375)], 2.5);
        t.simple("minecraft:ice", &["IceBlock"], cube(), 0.5);
        t.simple("minecraft:lily_pad", &["WaterlilyBlock"], vec![Aabb::new(0.0625, 0.0, 0.0625, 0.9375, 0.09375, 0.9375)], 0.0);
        for layers in 1..=8u8 {
            let h = (layers - 1) as f64 * 0.125;
            t.add(RawState {
                name: "minecraft:snow".into(),
                classes: vec!["SnowLayerBlock".into()],
                properties: vec![("layers".into(), layers.to_string())],
                hardness: 0.1,
                replaceable: layers == 1,
                collision: if h == 0.0 { vec![] } else { vec![Aabb::new(0.0, 0.0, 0.0, 1.0, h, 1.0)] },
                pathfindable_land: Some(layers < 5),
                item: "minecraft:snow".into(),
                ..Default::default()
            });
        }
        for (fluid, name) in [(Fluid::Water, "minecraft:water"), (Fluid::Lava, "minecraft:lava")] {
            for level in 0..16u8 {
                let amount = if level == 0 || level >= 8 { 8 } else { 8 - level };
                t.add(RawState {
                    name: name.into(),
                    classes: vec!["LiquidBlock".into()],
                    properties: vec![("level".into(), level.to_string())],
                    hardness: 100.0,
                    replaceable: true,
                    fluid,
                    fluid_amount: amount,
                    fluid_source: level == 0,
                    pathfindable_land: Some(false),
                    item: "minecraft:air".into(),
                    ..Default::default()
                });
            }
        }
        for ty in ["top", "bottom", "double"] {
            for wl in ["true", "false"] {
                let collision = match ty {
                    "top" => vec![Aabb::new(0.0, 0.5, 0.0, 1.0, 1.0, 1.0)],
                    "bottom" => vec![Aabb::new(0.0, 0.0, 0.0, 1.0, 0.5, 1.0)],
                    _ => cube(),
                };
                t.add(RawState {
                    name: "minecraft:stone_slab".into(),
                    classes: vec!["SlabBlock".into()],
                    properties: vec![("type".into(), ty.into()), ("waterlogged".into(), wl.into())],
                    hardness: 2.0,
                    requires_tool: true,
                    fluid: if wl == "true" { Fluid::Water } else { Fluid::None },
                    fluid_amount: if wl == "true" { 8 } else { 0 },
                    fluid_source: wl == "true",
                    collision,
                    pathfindable_land: Some(false),
                    item: "minecraft:stone_slab".into(),
                    ..Default::default()
                });
            }
        }
        for half in ["top", "bottom"] {
            t.add(RawState {
                name: "minecraft:oak_stairs".into(),
                classes: vec!["StairBlock".into()],
                properties: vec![
                    ("facing".into(), "north".into()),
                    ("half".into(), half.into()),
                    ("shape".into(), "straight".into()),
                    ("waterlogged".into(), "false".into()),
                ],
                hardness: 2.0,
                collision: if half == "bottom" {
                    vec![Aabb::new(0.0, 0.0, 0.0, 1.0, 0.5, 1.0), Aabb::new(0.0, 0.5, 0.0, 1.0, 1.0, 0.5)]
                } else {
                    vec![Aabb::new(0.0, 0.5, 0.0, 1.0, 1.0, 1.0), Aabb::new(0.0, 0.0, 0.0, 1.0, 0.5, 0.5)]
                },
                pathfindable_land: Some(false),
                item: "minecraft:oak_stairs".into(),
                ..Default::default()
            });
        }
        for (door, classes) in [("minecraft:oak_door", ["DoorBlock"]), ("minecraft:iron_door", ["DoorBlock"])] {
            for open in ["false", "true"] {
                for half in ["lower", "upper"] {
                    for facing in ["north", "south", "west", "east"] {
                        let thin = |f: &str| match f {
                            "north" => Aabb::new(0.0, 0.0, 0.8125, 1.0, 1.0, 1.0),
                            "south" => Aabb::new(0.0, 0.0, 0.0, 1.0, 1.0, 0.1875),
                            "west" => Aabb::new(0.8125, 0.0, 0.0, 1.0, 1.0, 1.0),
                            _ => Aabb::new(0.0, 0.0, 0.0, 0.1875, 1.0, 1.0),
                        };
                        let shape_facing = if open == "true" {
                            match facing {
                                "north" => "west",
                                "south" => "east",
                                "west" => "south",
                                _ => "north",
                            }
                        } else {
                            facing
                        };
                        t.add(RawState {
                            name: door.into(),
                            classes: classes.iter().map(|s| s.to_string()).collect(),
                            properties: vec![
                                ("facing".into(), facing.into()),
                                ("half".into(), half.into()),
                                ("hinge".into(), "left".into()),
                                ("open".into(), open.into()),
                                ("powered".into(), "false".into()),
                            ],
                            hardness: if door.contains("iron") { 5.0 } else { 3.0 },
                            collision: vec![thin(shape_facing)],
                            pathfindable_land: Some(open == "true"),
                            item: door.into(),
                            ..Default::default()
                        });
                    }
                }
            }
        }
        for open in ["false", "true"] {
            t.add(RawState {
                name: "minecraft:oak_fence_gate".into(),
                classes: vec!["FenceGateBlock".into()],
                properties: vec![
                    ("facing".into(), "north".into()),
                    ("in_wall".into(), "false".into()),
                    ("open".into(), open.into()),
                    ("powered".into(), "false".into()),
                ],
                hardness: 2.0,
                collision: if open == "true" { vec![] } else { vec![Aabb::new(0.0, 0.0, 0.375, 1.0, 1.5, 0.625)] },
                pathfindable_land: Some(open == "true"),
                item: "minecraft:oak_fence_gate".into(),
                ..Default::default()
            });
        }
        t.build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_the_movement_blocks() {
        let t = testing::vanilla();
        let get = |n: &str| t.get(t.default_state(n).unwrap());
        assert!(get("minecraft:air").is_air());
        assert!(get("minecraft:stone").is(flags::FULL_COLLISION));
        assert!(!get("minecraft:stone").is(flags::PATHFINDABLE_LAND));
        assert!(get("minecraft:short_grass").is(flags::PATHFINDABLE_LAND));
        assert!(get("minecraft:sand").is(flags::FALLING_BLOCK));
        assert!(get("minecraft:ladder").is(flags::CLIMBABLE));
        assert!(get("minecraft:oak_door").is(flags::DOOR));
        assert!(get("minecraft:iron_door").is(flags::IRON_DOOR));
        assert!(get("minecraft:oak_leaves").is(flags::LEAVES));
        assert_eq!(get("minecraft:water").liquid_level, 0);
        assert!(get("minecraft:water").is(flags::WATER_SOURCE_BLOCK));
        assert!(get("minecraft:white_carpet").is(flags::CARPET));
        assert!(!get("minecraft:soul_sand").is(flags::FULL_COLLISION));
        let slab = t
            .with_properties(t.block_id("stone_slab").unwrap(), &[("type", "top"), ("waterlogged", "false")])
            .unwrap();
        assert_eq!(t.get(slab).slab, SlabType::Top);
        let wl = t.with_property(slab, "waterlogged", "true").unwrap();
        assert!(t.get(wl).is(flags::WATERLOGGED));
        assert!(t.get(wl).is_water());
    }

    #[test]
    fn full_block_detection() {
        assert!(is_full_block(&[Aabb::new(0.0, 0.0, 0.0, 1.0, 1.0, 1.0)]));
        assert!(is_full_block(&[
            Aabb::new(0.0, 0.0, 0.0, 1.0, 0.5, 1.0),
            Aabb::new(0.0, 0.5, 0.0, 1.0, 1.0, 1.0)
        ]));
        assert!(!is_full_block(&[Aabb::new(0.0, 0.0, 0.0, 1.0, 0.5, 1.0)]));
        assert!(!is_full_block(&[]));
        assert!(!is_full_block(&[Aabb::new(0.0, 0.0, 0.0, 1.0, 1.5, 1.0)]));
    }
}
