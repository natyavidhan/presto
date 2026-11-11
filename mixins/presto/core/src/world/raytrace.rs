//! `BlockGetter.clip` with `ClipContext.Block.OUTLINE` and `ClipContext.Fluid.NONE`: what the crosshair picks
//! along a ray, walked block by block as vanilla's `traverseBlocks` does, against each state's outline boxes.
//! `RayTraceUtils` and `RotationUtils.reachable` build on it.

use super::{BlockStateInterface, HitResult};
use crate::math::{Aabb, BlockPos, Direction, Rotation, Vec3, mth, rotation_utils};

fn frac(v: f64) -> f64 {
    v - v.floor()
}

fn lerp(delta: f64, start: f64, end: f64) -> f64 {
    start + delta * (end - start)
}

fn sign(v: f64) -> i32 {
    if v == 0.0 {
        0
    } else if v > 0.0 {
        1
    } else {
        -1
    }
}

/// `Direction.getApproximateNearest`.
fn nearest(d: Vec3) -> Direction {
    let mut best = Direction::North;
    let mut best_dot = f64::MIN;
    for dir in Direction::ALL {
        let [x, y, z] = dir.normal();
        let dot = d.x * x as f64 + d.y * y as f64 + d.z * z as f64;
        if dot > best_dot {
            best_dot = dot;
            best = dir;
        }
    }
    best
}

/// `VoxelShape.clip` for a shape made of `boxes` at `pos`.
pub fn clip_shape(boxes: &[Aabb], from: Vec3, to: Vec3, pos: BlockPos) -> Option<HitResult> {
    if boxes.is_empty() {
        return None;
    }
    let d = to - from;
    if d.length_sqr() < 1.0e-7 {
        return None;
    }
    let nudged = from + d * 0.001;
    let local = nudged - pos.corner();
    if boxes.iter().any(|b| b.contains(local)) {
        return Some(HitResult::Block {
            pos,
            face: nearest(d).opposite(),
            location: nudged,
            inside: true,
        });
    }
    let mut best: Option<(f64, Vec3, Direction)> = None;
    for b in boxes {
        let moved = b.moved(pos.x as f64, pos.y as f64, pos.z as f64);
        if let Some((p, face)) = moved.clip(from, to) {
            let dist = from.distance_to_sqr(p);
            if best.is_none_or(|(bd, _, _)| dist < bd) {
                best = Some((dist, p, face));
            }
        }
    }
    best.map(|(_, location, face)| HitResult::Block {
        pos,
        face,
        location,
        inside: false,
    })
}

/// The first block outline the segment `from`..`to` meets, as `level.clip(new ClipContext(from, to, OUTLINE,
/// NONE, entity))` finds it.
pub fn clip(bsi: &BlockStateInterface, from: Vec3, to: Vec3) -> HitResult {
    if from == to {
        return HitResult::Miss;
    }
    let hit_at = |x: i32, y: i32, z: i32| -> Option<HitResult> {
        let info = bsi.info(x, y, z);
        clip_shape(&info.outline, from, to, BlockPos::new(x, y, z))
    };
    let d0 = lerp(-1.0e-7, to.x, from.x);
    let d1 = lerp(-1.0e-7, to.y, from.y);
    let d2 = lerp(-1.0e-7, to.z, from.z);
    let d3 = lerp(-1.0e-7, from.x, to.x);
    let d4 = lerp(-1.0e-7, from.y, to.y);
    let d5 = lerp(-1.0e-7, from.z, to.z);
    let mut i = mth::floor(d3);
    let mut j = mth::floor(d4);
    let mut k = mth::floor(d5);
    if let Some(hit) = hit_at(i, j, k) {
        return hit;
    }
    let d6 = d0 - d3;
    let d7 = d1 - d4;
    let d8 = d2 - d5;
    let l = sign(d6);
    let i1 = sign(d7);
    let j1 = sign(d8);
    let d9 = if l == 0 { f64::MAX } else { l as f64 / d6 };
    let d10 = if i1 == 0 { f64::MAX } else { i1 as f64 / d7 };
    let d11 = if j1 == 0 { f64::MAX } else { j1 as f64 / d8 };
    let mut d12 = d9 * if l > 0 { 1.0 - frac(d3) } else { frac(d3) };
    let mut d13 = d10 * if i1 > 0 { 1.0 - frac(d4) } else { frac(d4) };
    let mut d14 = d11 * if j1 > 0 { 1.0 - frac(d5) } else { frac(d5) };
    let mut guard = 0;
    while d12 <= 1.0 || d13 <= 1.0 || d14 <= 1.0 {
        guard += 1;
        if guard > 10_000 {
            break;
        }
        if d12 < d13 {
            if d12 < d14 {
                i += l;
                d12 += d9;
            } else {
                k += j1;
                d14 += d11;
            }
        } else if d13 < d14 {
            j += i1;
            d13 += d10;
        } else {
            k += j1;
            d14 += d11;
        }
        if let Some(hit) = hit_at(i, j, k) {
            return hit;
        }
    }
    HitResult::Miss
}

/// `RayTraceUtils.rayTraceTowards`: from the eyes, along a rotation, as far as `reach`.
pub fn ray_trace_towards(
    bsi: &BlockStateInterface,
    eyes: Vec3,
    rotation: Rotation,
    reach: f64,
) -> HitResult {
    let dir = rotation_utils::look_direction(rotation);
    let end = eyes + dir * reach;
    clip(bsi, eyes, end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::block::testing;
    use crate::world::{Dimension, MapSource, WorldBorder, WorldView};
    use std::sync::Arc;

    fn world(blocks: &[(BlockPos, &str)]) -> BlockStateInterface {
        let table = Arc::new(testing::vanilla());
        let mut src = MapSource {
            default: table.air(),
            ..Default::default()
        };
        for (p, n) in blocks {
            let state = if *n == "minecraft:stone_slab" {
                table
                    .with_properties(
                        table.block_id(n).unwrap(),
                        &[("type", "bottom"), ("waterlogged", "false")],
                    )
                    .unwrap()
            } else {
                table.default_state(n).unwrap()
            };
            src.set(*p, state);
        }
        let view = WorldView {
            table,
            source: Arc::new(src),
            cache: None,
            dimension: Arc::new(Dimension::overworld()),
            border: WorldBorder::default(),
        };
        BlockStateInterface::new(&view, true)
    }

    #[test]
    fn hits_the_face_it_looks_at() {
        let bsi = world(&[(BlockPos::new(2, 64, 0), "minecraft:stone")]);
        let eyes = Vec3::new(0.5, 64.5, 0.5);
        let hit = clip(&bsi, eyes, Vec3::new(5.5, 64.5, 0.5));
        match hit {
            HitResult::Block { pos, face, .. } => {
                assert_eq!(pos, BlockPos::new(2, 64, 0));
                assert_eq!(face, Direction::West);
            }
            other => panic!("{other:?}"),
        }
        // Looking down at the floor.
        let bsi = world(&[(BlockPos::new(0, 63, 0), "minecraft:stone")]);
        let hit = ray_trace_towards(&bsi, Vec3::new(0.5, 65.12, 0.5), Rotation::new(0.0, 90.0), 4.5);
        assert_eq!(hit.block_pos(), Some(BlockPos::new(0, 63, 0)));
        assert_eq!(hit.face(), Some(Direction::Up));
    }

    #[test]
    fn misses_through_air_and_respects_partial_shapes() {
        let bsi = world(&[(BlockPos::new(2, 64, 0), "minecraft:stone_slab")]);
        // The bottom slab's top half is empty.
        let hit = clip(&bsi, Vec3::new(0.5, 64.75, 0.5), Vec3::new(5.5, 64.75, 0.5));
        assert_eq!(hit, HitResult::Miss);
        let hit = clip(&bsi, Vec3::new(0.5, 64.25, 0.5), Vec3::new(5.5, 64.25, 0.5));
        assert_eq!(hit.block_pos(), Some(BlockPos::new(2, 64, 0)));
    }
}
