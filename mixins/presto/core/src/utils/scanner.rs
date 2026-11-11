//! `WorldScanner` (`FasterWorldScanner`): find blocks in the loaded chunks, nearest chunks first, scanning
//! chunks on every core at once. Sections whose palette lacks the wanted blocks are skipped by the source.

use super::block_optional_meta::StateSet;
use crate::math::BlockPos;
use crate::world::{BlockSource, Dimension};

/// `getChunkRange`: chunks in a square spiral out from the centre, nearest rings first.
pub fn chunk_range(center_x: i32, center_z: i32, chunk_radius: i32) -> Vec<(i32, i32)> {
    let mut chunks = vec![(center_x, center_z)];
    for i in 1..chunk_radius {
        for j in 0..=i {
            chunks.push((center_x - j, center_z - i));
            if j != 0 {
                chunks.push((center_x + j, center_z - i));
                chunks.push((center_x - j, center_z + i));
            }
            chunks.push((center_x + j, center_z + i));
            if j != i {
                chunks.push((center_x - i, center_z - j));
                chunks.push((center_x + i, center_z - j));
                if j != 0 {
                    chunks.push((center_x - i, center_z + j));
                    chunks.push((center_x + i, center_z + j));
                }
            }
        }
    }
    chunks
}

/// Scans one chunk, its sections from the player's height outward (`collectChunkSections`).
fn scan_chunk(
    src: &dyn BlockSource,
    dim: &Dimension,
    wanted: &StateSet,
    cx: i32,
    cz: i32,
    player_section: i32,
    out: &mut Vec<BlockPos>,
) {
    let sections = (dim.height + 15) >> 4;
    let mut i = player_section - 1;
    let mut j = player_section;
    while i >= 0 || j < sections {
        if j >= 0 && j < sections && !src.scan_section(cx, j, cz, dim.min_y, wanted, out) {
            return; // not loaded
        }
        if i >= 0 && i < sections && !src.scan_section(cx, i, cz, dim.min_y, wanted, out) {
            return;
        }
        j += 1;
        i -= 1;
    }
}

/// `scanChunkRadius`: up to `max` (all if negative) wanted blocks within `max_search_radius` chunks,
/// nearest chunks first. Chunks are scanned in parallel; the result keeps their nearest-first order.
pub fn scan_chunk_radius(
    src: &dyn BlockSource,
    dim: &Dimension,
    wanted: &StateSet,
    feet: BlockPos,
    max: i32,
    max_search_radius: i32,
) -> Vec<BlockPos> {
    if wanted.is_empty() {
        return Vec::new();
    }
    let chunks: Vec<(i32, i32)> = chunk_range(feet.x >> 4, feet.z >> 4, max_search_radius.max(0))
        .into_iter()
        .filter(|(cx, cz)| src.is_loaded(cx << 4, cz << 4))
        .collect();
    let player_section = (feet.y - dim.min_y) >> 4;
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(1, 16);
    let per_thread = chunks.len().div_ceil(threads).max(1);
    let mut results: Vec<Vec<Vec<BlockPos>>> = Vec::new();
    std::thread::scope(|scope| {
        let handles: Vec<_> = chunks
            .chunks(per_thread)
            .map(|part| {
                scope.spawn(move || {
                    part.iter()
                        .map(|(cx, cz)| {
                            let mut out = Vec::new();
                            scan_chunk(src, dim, wanted, *cx, *cz, player_section, &mut out);
                            out
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        for h in handles {
            results.push(h.join().unwrap_or_default());
        }
    });
    let mut out = Vec::new();
    for part in results {
        for chunk in part {
            out.extend(chunk);
            if max >= 0 && out.len() >= max as usize {
                out.truncate(max as usize);
                return out;
            }
        }
    }
    out
}

/// `scanChunk`: up to `max` wanted blocks in one chunk, bottom up.
pub fn scan_chunk_at(src: &dyn BlockSource, dim: &Dimension, wanted: &StateSet, cx: i32, cz: i32, max: i32) -> Vec<BlockPos> {
    let mut out = Vec::new();
    let sections = (dim.height + 15) >> 4;
    for sy in 0..sections {
        if !src.scan_section(cx, sy, cz, dim.min_y, wanted, &mut out) {
            break;
        }
        if max >= 0 && out.len() >= max as usize {
            out.truncate(max as usize);
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::block::testing;
    use crate::world::MapSource;

    #[test]
    fn spiral_starts_at_centre_and_covers_the_square() {
        let r = chunk_range(0, 0, 3);
        assert_eq!(r[0], (0, 0));
        let mut sorted = r.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), r.len());
        assert_eq!(r.len(), 25);
    }

    #[test]
    fn finds_blocks_nearest_first() {
        let t = testing::vanilla();
        let gold = t.default_state("minecraft:chest").unwrap();
        let mut src = MapSource {
            default: t.air(),
            ..Default::default()
        };
        src.set(BlockPos::new(100, 10, 100), gold);
        src.set(BlockPos::new(3, 64, 3), gold);
        src.loaded.insert((0, 0));
        src.loaded.insert((6, 6));
        let mut wanted = StateSet::with_capacity(t.len());
        wanted.insert(gold);
        let dim = Dimension::overworld();
        let found = scan_chunk_radius(&src, &dim, &wanted, BlockPos::new(0, 64, 0), -1, 10);
        assert_eq!(found.first(), Some(&BlockPos::new(3, 64, 3)));
        assert!(found.contains(&BlockPos::new(100, 10, 100)));
        let one = scan_chunk_radius(&src, &dim, &wanted, BlockPos::new(0, 64, 0), 1, 10);
        assert_eq!(one, vec![BlockPos::new(3, 64, 3)]);
    }
}
