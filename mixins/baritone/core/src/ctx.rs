//! `IPlayerContext` for one tick: the player, the live world (through a [`CalculationContext`], so every
//! `MovementHelper` rule reads it the same way the search did), and the world-dependent parts of
//! `RotationUtils` and `VecUtils`.

use crate::behavior::look::AimProcessor;
use crate::math::{BlockPos, Rotation, Vec3, rotation_utils};
use crate::pathing::context::CalculationContext;
use crate::player::{GameState, Inventory, PlayerState};
use crate::settings::Settings;
use crate::world::{HitResult, flags, raytrace};
use std::sync::Arc;

/// The player and the world as they are this tick.
pub struct PlayerContext<'a> {
    pub game: &'a GameState,
    pub player: &'a PlayerState,
    pub settings: &'a Arc<Settings>,
    /// The live world, with the costs and rules of a fresh `CalculationContext`.
    pub world: CalculationContext,
}

impl<'a> PlayerContext<'a> {
    /// `None` outside a world.
    pub fn new(game: &'a GameState, settings: &'a Arc<Settings>, has_throwaway: bool) -> Option<Self> {
        let player = game.player.as_ref()?;
        let world = CalculationContext::new(game, settings, has_throwaway, false)?;
        Some(Self {
            game,
            player,
            settings,
            world,
        })
    }

    pub fn inventory(&self) -> &Inventory {
        &self.game.inventory
    }

    /// `playerFeet`: the block at the feet (a little above them, for soul sand), or the one above when that is
    /// a slab the player stands in.
    pub fn player_feet(&self) -> BlockPos {
        let feet = self.player.feet_raw();
        if self.world.info_pos(feet).is(flags::SLAB) {
            return feet.above();
        }
        feet
    }

    pub fn player_feet_vec(&self) -> Vec3 {
        self.player.position
    }

    pub fn player_head(&self) -> Vec3 {
        self.player.head()
    }

    pub fn player_motion(&self) -> Vec3 {
        self.player.velocity
    }

    pub fn player_rotations(&self) -> Rotation {
        self.player.rotation()
    }

    /// What the crosshair is on (`objectMouseOver`).
    pub fn object_mouse_over(&self) -> HitResult {
        self.game.hit
    }

    pub fn selected_block(&self) -> Option<BlockPos> {
        self.game.hit.block_pos()
    }

    pub fn is_looking_at(&self, pos: BlockPos) -> bool {
        self.selected_block() == Some(pos)
    }

    /// `IPlayerController.getBlockReachDistance`: 5 in creative, else `blockReachDistance`.
    pub fn block_reach_distance(&self) -> f64 {
        if self.player.game_mode.is_creative() {
            5.0
        } else {
            self.settings.block_reach_distance as f64
        }
    }

    /// `RayTraceUtils.rayTraceTowards`.
    pub fn ray_trace_towards(&self, rotation: Rotation, reach: f64, would_sneak: bool) -> HitResult {
        let eyes = if would_sneak {
            self.player.sneaking_eyes()
        } else {
            self.player.eyes()
        };
        raytrace::ray_trace_towards(&self.world.bsi, eyes, rotation, reach)
    }

    /// `VecUtils.calculateBlockCenter`: the middle of the block's collision shape (its bottom for fire).
    pub fn calculate_block_center(&self, pos: BlockPos) -> Vec3 {
        let info = self.world.info_pos(pos);
        let Some(b) = info.collision_bounds() else {
            return pos.center();
        };
        let mut y = (b.min.y + b.max.y) / 2.0;
        if info.is(flags::FIRE) {
            y = 0.0; // look at the bottom of fire when putting it out
        }
        Vec3::new(
            pos.x as f64 + (b.min.x + b.max.x) / 2.0,
            pos.y as f64 + y,
            pos.z as f64 + (b.min.z + b.max.z) / 2.0,
        )
    }

    /// `RotationUtils.reachable`: a rotation from which the crosshair would be on `pos`, if any.
    pub fn reachable(&self, aim: &AimProcessor, pos: BlockPos, reach: f64, would_sneak: bool) -> Option<Rotation> {
        if self.settings.remain_with_existing_look_direction && self.is_looking_at(pos) {
            // The same view, a hair down, so it isn't "no change".
            let hypothetical = self.player_rotations().add(Rotation::new(0.0, 0.0001));
            if would_sneak {
                // Would we still be looking at it once crouched?
                let r = self.ray_trace_towards(hypothetical, reach, true);
                if r.block_pos() == Some(pos) {
                    return Some(hypothetical);
                }
            } else {
                return Some(hypothetical);
            }
        }
        if let Some(r) = self.reachable_offset(aim, pos, self.calculate_block_center(pos), reach, would_sneak) {
            return Some(r);
        }
        let info = self.world.info_pos(pos);
        let shape = info
            .outline_bounds()
            .unwrap_or(crate::math::Aabb::new(0.0, 0.0, 0.0, 1.0, 1.0, 1.0));
        // `BLOCK_SIDE_MULTIPLIERS`: the middle of each face (down, up, north, south, west, east).
        const SIDES: [[f64; 3]; 6] = [
            [0.5, 0.0, 0.5],
            [0.5, 1.0, 0.5],
            [0.5, 0.5, 0.0],
            [0.5, 0.5, 1.0],
            [0.0, 0.5, 0.5],
            [1.0, 0.5, 0.5],
        ];
        for side in SIDES {
            let x = shape.min.x * side[0] + shape.max.x * (1.0 - side[0]);
            let y = shape.min.y * side[1] + shape.max.y * (1.0 - side[1]);
            let z = shape.min.z * side[2] + shape.max.z * (1.0 - side[2]);
            let target = pos.corner() + Vec3::new(x, y, z);
            if let Some(r) = self.reachable_offset(aim, pos, target, reach, would_sneak) {
                return Some(r);
            }
        }
        None
    }

    /// `RotationUtils.reachableOffset`.
    pub fn reachable_offset(
        &self,
        aim: &AimProcessor,
        pos: BlockPos,
        offset: Vec3,
        reach: f64,
        would_sneak: bool,
    ) -> Option<Rotation> {
        let eyes = if would_sneak {
            self.player.sneaking_eyes()
        } else {
            self.player.eyes()
        };
        let rotation = rotation_utils::calc_rotation_from_vec3d_rel(eyes, offset, self.player_rotations());
        let actual = aim.peek_rotation(rotation, self.player_rotations(), self.game.mouse_sensitivity);
        let result = self.ray_trace_towards(actual, reach, would_sneak);
        if let HitResult::Block { pos: hit, .. } = result {
            if hit == pos {
                return Some(rotation);
            }
            if self.world.info_pos(pos).is(flags::FIRE) && hit == pos.below() {
                return Some(rotation);
            }
        }
        None
    }

    /// `RotationUtils.calcRotationFromVec3d(playerHead, dest, playerRotations)`.
    pub fn rotation_to(&self, dest: Vec3) -> Rotation {
        rotation_utils::calc_rotation_from_vec3d_rel(self.player_head(), dest, self.player_rotations())
    }

    /// The blocks the player's feet stand over (`MovementHelper.steppingOnBlocks`).
    pub fn stepping_on_blocks(&self) -> Vec<BlockPos> {
        let feet = BlockPos::of_vec(self.player.position);
        let bb = self.player.bounding_box;
        let mut out = Vec::new();
        for x in -1..=1 {
            for z in -1..=1 {
                let cell = crate::math::Aabb::new(
                    (feet.x + x) as f64,
                    feet.y as f64,
                    (feet.z + z) as f64,
                    (feet.x + x + 1) as f64,
                    (feet.y + 1) as f64,
                    (feet.z + z + 1) as f64,
                );
                if bb.intersects(cell) {
                    out.push(BlockPos::new(feet.x + x, feet.y - 1, feet.z + z));
                }
            }
        }
        out
    }
}
