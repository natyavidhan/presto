//! `MovementHelper`: what Baritone may walk on, walk through, break and place against, and what each costs.
//! Every function reads blocks through a [`CalculationContext`], whether the path-finding threads call it or
//! a movement checks the live world while it runs.

use super::context::CalculationContext;
use super::costs::COST_INF;
use super::precomputed::Ternary;
use crate::math::{BlockPos, Direction};
use crate::world::{BlockInfo, Fluid, SlabType, StateId, flags};

/// `MovementHelper.avoidBreaking`.
pub fn avoid_breaking(ctx: &CalculationContext, x: i32, y: i32, z: i32, info: &BlockInfo) -> bool {
    if !ctx.world_border.can_place_at(x, z) {
        return true;
    }
    ctx.blocks_to_disallow_breaking.contains(&info.block)
        || info.is(flags::ICE) // ice becomes water, and water can mess up the path
        || info.is(flags::INFESTED)
        || avoid_adjacent_breaking(ctx, x, y + 1, z, true)
        || avoid_adjacent_breaking(ctx, x + 1, y, z, false)
        || avoid_adjacent_breaking(ctx, x - 1, y, z, false)
        || avoid_adjacent_breaking(ctx, x, y, z + 1, false)
        || avoid_adjacent_breaking(ctx, x, y, z - 1, false)
}

/// `MovementHelper.avoidAdjacentBreaking`: lava or water that would flow in if this block went, or an
/// unsupported falling block beside it.
pub fn avoid_adjacent_breaking(
    ctx: &CalculationContext,
    x: i32,
    y: i32,
    z: i32,
    directly_above: bool,
) -> bool {
    let info = ctx.info(x, y, z);
    if !directly_above
        && info.is(flags::FALLING_BLOCK)
        && ctx.settings.avoid_updating_falling_blocks
        && is_free(ctx.info(x, y - 1, z))
    {
        return true;
    }
    // Only pure liquids for now: waterlogged blocks can have closed bottom sides.
    if info.is(flags::LIQUID_BLOCK) {
        if directly_above || ctx.settings.strict_liquid_check {
            return true;
        }
        if info.liquid_level == 0 {
            return true; // source blocks like to flow horizontally
        }
        // Everything else prefers flowing down.
        return !ctx.info(x, y - 1, z).is(flags::LIQUID_BLOCK);
    }
    info.has_fluid()
}

/// `FallingBlock.isFree`: air, fire, a liquid or a replaceable block below lets a falling block fall.
pub fn is_free(below: &BlockInfo) -> bool {
    below.is_air() || below.is(flags::FIRE) || below.has_fluid() || below.is(flags::REPLACEABLE)
}

#[inline]
pub fn can_walk_through(ctx: &CalculationContext, x: i32, y: i32, z: i32) -> bool {
    let state = ctx.get(x, y, z);
    ctx.precomputed.can_walk_through(ctx, x, y, z, state)
}

#[inline]
pub fn can_walk_through_state(ctx: &CalculationContext, x: i32, y: i32, z: i32, state: StateId) -> bool {
    ctx.precomputed.can_walk_through(ctx, x, y, z, state)
}

pub fn can_walk_through_pos(ctx: &CalculationContext, pos: BlockPos) -> bool {
    can_walk_through(ctx, pos.x, pos.y, pos.z)
}

/// `canWalkThroughBlockState`.
pub fn can_walk_through_block_state(ctx: &CalculationContext, info: &BlockInfo) -> Ternary {
    if info.is_air() {
        return Ternary::Yes;
    }
    if info.is(flags::FIRE)
        || info.is(flags::COBWEB)
        || info.is(flags::END_PORTAL)
        || info.is(flags::COCOA)
        || info.is(flags::ABSTRACT_SKULL)
        || info.is(flags::BUBBLE_COLUMN)
        || info.is(flags::SHULKER_BOX)
        || info.is(flags::SLAB)
        || info.is(flags::TRAPDOOR)
        || info.is(flags::HONEY_BLOCK)
        || info.is(flags::END_ROD)
        || info.is(flags::SWEET_BERRY_BUSH)
        || info.is(flags::POINTED_DRIPSTONE)
        || info.is(flags::AMETHYST_CLUSTER)
        || info.is(flags::AZALEA)
    {
        return Ternary::No;
    }
    if info.is(flags::BIG_DRIPLEAF) || info.is(flags::POWDER_SNOW) {
        return Ternary::No;
    }
    if ctx.blocks_to_avoid.contains(&info.block) {
        return Ternary::No;
    }
    if info.is(flags::DOOR) || info.is(flags::FENCE_GATE) {
        // This assumes every door can be opened.
        if info.is(flags::IRON_DOOR) {
            return Ternary::No;
        }
        return Ternary::Yes;
    }
    if info.is(flags::CARPET) {
        return Ternary::Maybe;
    }
    if info.is(flags::SNOW_LAYER) {
        // Snow cached as a chunk's top block has no layers to read: decide by position.
        return Ternary::Maybe;
    }
    if info.has_fluid() {
        if info.fluid_amount != 8 {
            return Ternary::No;
        }
        return Ternary::Maybe;
    }
    if info.is(flags::CAULDRON) {
        return Ternary::No;
    }
    if info.is(flags::PATHFINDABLE_LAND) {
        Ternary::Yes
    } else {
        Ternary::No
    }
}

/// `canWalkThroughPosition`.
pub fn can_walk_through_position(
    ctx: &CalculationContext,
    x: i32,
    y: i32,
    z: i32,
    info: &BlockInfo,
) -> bool {
    if info.is(flags::CARPET) {
        return can_walk_on(ctx, x, y - 1, z);
    }
    if info.is(flags::SNOW_LAYER) {
        // Cached as a top block we don't know its layers: say yes, or long paths through snowy biomes fail.
        if !ctx.bsi.world_contains_loaded_chunk(x, z) {
            return true;
        }
        // BlockSnow.isPassable is layers < 5, but 3 or more is impassable under a two-high ceiling.
        if info.snow_layers >= 3 {
            return false;
        }
        return can_walk_on(ctx, x, y - 1, z);
    }
    if info.has_fluid() {
        if is_flowing(ctx, x, y, z, info) {
            return false;
        }
        // Everything past here relies on the water being still.
        if ctx.settings.assume_walk_on_water {
            return false;
        }
        let up = ctx.info(x, y + 1, z);
        if up.has_fluid() || up.is(flags::LILY_PAD) {
            return false;
        }
        return info.fluid == Fluid::Water;
    }
    info.is(flags::PATHFINDABLE_LAND)
}

/// `fullyPassableBlockState`.
pub fn fully_passable_block_state(info: &BlockInfo) -> Ternary {
    if info.is_air() {
        return Ternary::Yes;
    }
    // Passable, but not something to jump through.
    if info.is(flags::FIRE)
        || info.is(flags::TRIPWIRE)
        || info.is(flags::COBWEB)
        || info.is(flags::VINE)
        || info.is(flags::LADDER)
        || info.is(flags::COCOA)
        || info.is(flags::AZALEA)
        || info.is(flags::DOOR)
        || info.is(flags::FENCE_GATE)
        || info.is(flags::SNOW_LAYER)
        || info.has_fluid()
        || info.is(flags::TRAPDOOR)
        || info.is(flags::END_PORTAL_CLASS)
        || info.is(flags::SKULL)
        || info.is(flags::SHULKER_BOX)
    {
        return Ternary::No;
    }
    if info.is(flags::PATHFINDABLE_LAND) {
        Ternary::Yes
    } else {
        Ternary::No
    }
}

/// `fullyPassable`: walk through without slowing down (no doors to open, no water, no ladders or cobwebs).
#[inline]
pub fn fully_passable(ctx: &CalculationContext, x: i32, y: i32, z: i32) -> bool {
    ctx.precomputed.fully_passable(ctx, ctx.get(x, y, z))
}

#[inline]
pub fn fully_passable_state(ctx: &CalculationContext, state: StateId) -> bool {
    ctx.precomputed.fully_passable(ctx, state)
}

/// `isReplaceable`, for MovementTraverse and MovementAscend.
pub fn is_replaceable(ctx: &CalculationContext, x: i32, y: i32, z: i32, info: &BlockInfo) -> bool {
    if info.is_air() {
        return true;
    }
    if info.is(flags::SNOW_LAYER) {
        if !ctx.bsi.world_contains_loaded_chunk(x, z) {
            return true;
        }
        return info.snow_layers == 1;
    }
    if info.is(flags::TALL_GRASS) {
        return true;
    }
    info.is(flags::REPLACEABLE)
}

/// `isDoorPassable`.
pub fn is_door_passable(ctx: &CalculationContext, door: BlockPos, player: BlockPos) -> bool {
    if player == door {
        return false;
    }
    let info = ctx.info_pos(door);
    if !info.is(flags::DOOR) {
        return true;
    }
    is_horizontal_block_passable(door, info, player)
}

/// `isGatePassable`.
pub fn is_gate_passable(ctx: &CalculationContext, gate: BlockPos, player: BlockPos) -> bool {
    if player == gate {
        return false;
    }
    let info = ctx.info_pos(gate);
    if !info.is(flags::FENCE_GATE) {
        return true;
    }
    info.open
}

/// `isHorizontalBlockPassable`: a door is passable from the side its leaf doesn't block.
pub fn is_horizontal_block_passable(block: BlockPos, info: &BlockInfo, player: BlockPos) -> bool {
    if player == block {
        return false;
    }
    let Some(facing) = info.facing else {
        return true;
    };
    let facing_axis = facing.axis();
    let player_axis = if player.north() == block || player.south() == block {
        crate::math::Axis::Z
    } else if player.east() == block || player.west() == block {
        crate::math::Axis::X
    } else {
        return true;
    };
    (facing_axis == player_axis) == info.open
}

/// `avoidWalkingInto`.
pub fn avoid_walking_into(ctx: &CalculationContext, info: &BlockInfo) -> bool {
    crate::cache::chunk::avoid_walking_into(info, ctx.allow_walk_on_magma_blocks)
}

#[inline]
pub fn can_walk_on(ctx: &CalculationContext, x: i32, y: i32, z: i32) -> bool {
    let state = ctx.get(x, y, z);
    ctx.precomputed.can_walk_on(ctx, x, y, z, state)
}

#[inline]
pub fn can_walk_on_state(ctx: &CalculationContext, x: i32, y: i32, z: i32, state: StateId) -> bool {
    ctx.precomputed.can_walk_on(ctx, x, y, z, state)
}

pub fn can_walk_on_pos(ctx: &CalculationContext, pos: BlockPos) -> bool {
    can_walk_on(ctx, pos.x, pos.y, pos.z)
}

/// `canWalkOnBlockState`.
pub fn can_walk_on_block_state(ctx: &CalculationContext, info: &BlockInfo) -> Ternary {
    if is_block_normal_cube(info)
        && (!info.is(flags::MAGMA) || ctx.settings.allow_walk_on_magma_blocks)
        && !info.is(flags::BUBBLE_COLUMN)
        && !info.is(flags::HONEY_BLOCK)
    {
        return Ternary::Yes;
    }
    if info.is(flags::AZALEA) {
        return Ternary::Yes;
    }
    if info.is(flags::LADDER) || (is_climbable(info) && ctx.settings.allow_vines) {
        return Ternary::Yes;
    }
    if info.is(flags::FARMLAND) || info.is(flags::DIRT_PATH) || info.is(flags::SOUL_SAND) {
        return Ternary::Yes;
    }
    if info.is(flags::CHEST) {
        return Ternary::Yes;
    }
    if info.is(flags::GLASS) || info.is(flags::STAINED_GLASS) {
        return Ternary::Yes;
    }
    if info.is(flags::STAIRS) {
        return Ternary::Yes;
    }
    if info.is_water() {
        return Ternary::Maybe;
    }
    if info.is_lava() && ctx.settings.assume_walk_on_lava {
        return Ternary::Maybe;
    }
    if info.is(flags::SLAB) {
        if !ctx.settings.allow_walk_on_bottom_slab {
            if info.slab != SlabType::Bottom {
                return Ternary::Yes;
            }
            return Ternary::No;
        }
        return Ternary::Yes;
    }
    Ternary::No
}

/// `canWalkOnPosition`.
pub fn can_walk_on_position(ctx: &CalculationContext, x: i32, y: i32, z: i32, info: &BlockInfo) -> bool {
    if info.is_water() {
        let up = ctx.info(x, y + 1, z);
        if up.is(flags::LILY_PAD) || up.is(flags::CARPET) {
            return true;
        }
        if is_flowing(ctx, x, y, z, info) || (up.is_water() && !up.fluid_source) {
            // The only flowing water to walk on is under still water with jesus off.
            return up.is_water() && !ctx.settings.assume_walk_on_water;
        }
        // With assumeWalkOnWater, only water with nothing above; without it, only water under water.
        return up.is_water() ^ ctx.settings.assume_walk_on_water;
    }
    if info.is_lava() && !is_flowing(ctx, x, y, z, info) && ctx.settings.assume_walk_on_lava {
        return true;
    }
    false
}

/// `canUseFrostWalker`: still water at a source that Frost Walker will freeze.
pub fn can_use_frost_walker(ctx: &CalculationContext, info: &BlockInfo) -> bool {
    ctx.frost_walker != 0 && info.is(flags::WATER_SOURCE_BLOCK) && info.liquid_level == 0
}

/// `mustBeSolidToWalkOn`: will standing on this block leave us on top of something?
pub fn must_be_solid_to_walk_on(
    ctx: &CalculationContext,
    x: i32,
    y: i32,
    z: i32,
    info: &BlockInfo,
) -> bool {
    if is_climbable(info) {
        return false;
    }
    if info.has_fluid() {
        // For Frost Walker: only the blocks where we are still on the ground leaving to any side.
        if info.is(flags::SLAB) {
            if info.slab != SlabType::Bottom {
                return true;
            }
        } else if info.is(flags::STAIRS) {
            if info.half_top {
                return true;
            }
            if info.stair_inner {
                return true;
            }
        } else if info.is(flags::TRAPDOOR) {
            if !info.open && info.half_top {
                return true;
            }
        } else if info.is(flags::SCAFFOLDING) || info.is(flags::LEAVES) {
            return true;
        }
        if ctx.assume_walk_on_water {
            return false;
        }
        if ctx.info(x, y + 1, z).is(flags::LIQUID_BLOCK) {
            return false;
        }
    }
    true
}

/// `canPlaceAgainst`: can we look at the middle of a side of this block and expect to place there?
pub fn can_place_against(ctx: &CalculationContext, x: i32, y: i32, z: i32) -> bool {
    can_place_against_info(ctx, x, z, ctx.info(x, y, z))
}

pub fn can_place_against_pos(ctx: &CalculationContext, pos: BlockPos) -> bool {
    can_place_against(ctx, pos.x, pos.y, pos.z)
}

pub fn can_place_against_info(ctx: &CalculationContext, x: i32, z: i32, info: &BlockInfo) -> bool {
    if !ctx.world_border.can_place_at(x, z) {
        return false;
    }
    is_block_normal_cube(info) || info.is(flags::GLASS) || info.is(flags::STAINED_GLASS)
}

/// `isClimbable`: ladders and vines, climbed by holding jump inside them.
#[inline]
pub fn is_climbable(info: &BlockInfo) -> bool {
    info.is(flags::CLIMBABLE)
}

pub fn get_mining_duration_ticks(
    ctx: &CalculationContext,
    x: i32,
    y: i32,
    z: i32,
    include_falling: bool,
) -> f64 {
    get_mining_duration_ticks_state(ctx, x, y, z, ctx.get(x, y, z), include_falling)
}

/// `getMiningDurationTicks`: the ticks to clear this block (0 if it needn't be), `COST_INF` if it can't be.
pub fn get_mining_duration_ticks_state(
    ctx: &CalculationContext,
    x: i32,
    y: i32,
    z: i32,
    state: StateId,
    include_falling: bool,
) -> f64 {
    if can_walk_through_state(ctx, x, y, z, state) {
        return 0.0; // we won't mine it, so we don't check the falling blocks above either
    }
    let info = ctx.table.get(state);
    if info.has_fluid() {
        return COST_INF;
    }
    let mult = ctx.break_cost_multiplier_at(x, y, z, info);
    if mult >= COST_INF {
        return COST_INF;
    }
    if avoid_breaking(ctx, x, y, z, info) {
        return COST_INF;
    }
    let strength = ctx.tool_set.str_vs_block(info);
    if strength <= 0.0 {
        return COST_INF;
    }
    let mut result = 1.0 / strength;
    result += ctx.break_block_additional_cost;
    result *= mult;
    if include_falling {
        let above = ctx.get(x, y + 1, z);
        if ctx.table.get(above).is(flags::FALLING_BLOCK) {
            result += get_mining_duration_ticks_state(ctx, x, y + 1, z, above, true);
        }
    }
    result
}

#[inline]
pub fn is_bottom_slab(info: &BlockInfo) -> bool {
    info.is(flags::SLAB) && info.slab == SlabType::Bottom
}

#[inline]
pub fn is_water(info: &BlockInfo) -> bool {
    info.is_water()
}

#[inline]
pub fn is_lava(info: &BlockInfo) -> bool {
    info.is_lava()
}

#[inline]
pub fn is_liquid(info: &BlockInfo) -> bool {
    info.has_fluid()
}

/// `possiblyFlowing`: a flowing fluid that isn't at its full height.
pub fn possibly_flowing(info: &BlockInfo) -> bool {
    info.has_fluid() && info.fluid_amount != 8
}

/// `isFlowing`: not full, or beside fluid that isn't.
pub fn is_flowing(ctx: &CalculationContext, x: i32, y: i32, z: i32, info: &BlockInfo) -> bool {
    if !info.has_fluid() {
        return false;
    }
    if info.fluid_amount != 8 {
        return true;
    }
    possibly_flowing(ctx.info(x + 1, y, z))
        || possibly_flowing(ctx.info(x - 1, y, z))
        || possibly_flowing(ctx.info(x, y, z + 1))
        || possibly_flowing(ctx.info(x, y, z - 1))
}

/// `isBlockNormalCube`: a full collision cube that isn't one of the shapes that only look like one.
pub fn is_block_normal_cube(info: &BlockInfo) -> bool {
    if info.is(flags::BAMBOO_STALK)
        || info.is(flags::MOVING_PISTON)
        || info.is(flags::SCAFFOLDING)
        || info.is(flags::SHULKER_BOX)
        || info.is(flags::POINTED_DRIPSTONE)
        || info.is(flags::AMETHYST_CLUSTER)
    {
        return false;
    }
    info.is(flags::FULL_COLLISION)
}

/// `isTransparent`: air, or the water or lava block at any level.
pub fn is_transparent(info: &BlockInfo) -> bool {
    info.is_air() || info.is(flags::LIQUID_BLOCK)
}

/// The five directions `Movement.HORIZONTALS_BUT_ALSO_DOWN_____SO_EVERY_DIRECTION_EXCEPT_UP` lists.
pub const HORIZONTALS_AND_DOWN: [Direction; 5] = Direction::HORIZONTALS_AND_DOWN;
