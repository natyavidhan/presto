//! `CachedRegion`: 32 by 32 cached chunks, saved as one gzipped `r.<x>.<z>.bcr` file in the format Java
//! Baritone writes, so a cache made by either reads in the other.

use super::chunk::{self, CachedChunk};
use crate::math::BlockPos;
use crate::world::{BlockTable, Dimension, StateId};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const CHUNK_NOT_PRESENT: u8 = 0;
const CHUNK_PRESENT: u8 = 1;
const CACHED_REGION_MAGIC: i32 = 456022911;

/// A region: immutable once made, replaced whole when a chunk in it changes.
#[derive(Clone, Debug)]
pub struct CachedRegion {
    pub x: i32,
    pub z: i32,
    chunks: Vec<Option<Arc<CachedChunk>>>,
    pub dimension: Arc<Dimension>,
}

impl CachedRegion {
    pub fn empty(x: i32, z: i32, dimension: Arc<Dimension>) -> Self {
        Self {
            x,
            z,
            chunks: vec![None; 32 * 32],
            dimension,
        }
    }

    #[inline]
    fn slot(cx: i32, cz: i32) -> usize {
        (cx as usize) * 32 + cz as usize
    }

    /// The state at region-relative `x, z` (0..512) and absolute `y`, if the chunk is cached.
    #[inline]
    pub fn block(&self, x: i32, y: i32, z: i32, dim: &Dimension, table: &BlockTable) -> Option<StateId> {
        let adj = y - dim.min_y;
        self.chunks[Self::slot(x >> 4, z >> 4)]
            .as_ref()
            .map(|c| c.get_block(x & 15, adj, z & 15, dim, table))
    }

    pub fn is_cached(&self, x: i32, z: i32) -> bool {
        self.chunks[Self::slot(x >> 4, z >> 4)].is_some()
    }

    pub fn chunk(&self, cx: i32, cz: i32) -> Option<&Arc<CachedChunk>> {
        self.chunks[Self::slot(cx, cz)].as_ref()
    }

    /// A copy with one chunk replaced (`updateCachedChunk`).
    pub fn with_chunk(&self, cx: i32, cz: i32, chunk: Option<Arc<CachedChunk>>) -> CachedRegion {
        let mut next = self.clone();
        next.chunks[Self::slot(cx, cz)] = chunk;
        next
    }

    /// Every tracked block of one kind in this region (`getLocationsOf`).
    pub fn locations_of(&self, block: &str) -> Vec<BlockPos> {
        self.chunks
            .iter()
            .flatten()
            .flat_map(|c| c.absolute_blocks(block))
            .collect()
    }

    /// The chunks cached longer ago than `expiry_seconds` dropped (`removeExpired`); `None` if none were.
    pub fn without_expired(&self, expiry_seconds: i64, now_ms: i64) -> Option<CachedRegion> {
        if expiry_seconds < 0 {
            return None;
        }
        let oldest = now_ms - expiry_seconds * 1000;
        if !self
            .chunks
            .iter()
            .flatten()
            .any(|c| c.cache_timestamp < oldest)
        {
            return None;
        }
        let mut next = self.clone();
        for c in next.chunks.iter_mut() {
            if c.as_ref().is_some_and(|c| c.cache_timestamp < oldest) {
                *c = None;
            }
        }
        Some(next)
    }

    /// The chunk cached most recently (`mostRecentlyModified`).
    pub fn most_recently_modified(&self) -> Option<&Arc<CachedChunk>> {
        self.chunks
            .iter()
            .flatten()
            .max_by_key(|c| c.cache_timestamp)
    }

    pub fn file(dir: &Path, x: i32, z: i32) -> PathBuf {
        dir.join(format!("r.{x}.{z}.bcr"))
    }

    /// Writes the region to `dir` (`save`).
    pub fn save(&self, dir: &Path) -> io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let path = Self::file(dir, self.x, self.z);
        let tmp = path.with_extension("bcr.tmp");
        {
            let file = std::fs::File::create(&tmp)?;
            let mut out = flate2::write::GzEncoder::new(
                io::BufWriter::with_capacity(16384, file),
                flate2::Compression::default(),
            );
            out.write_all(&CACHED_REGION_MAGIC.to_be_bytes())?;
            let expected = chunk::size_in_bytes(chunk::size(self.dimension.height));
            for x in 0..32 {
                for z in 0..32 {
                    match &self.chunks[Self::slot(x, z)] {
                        None => out.write_all(&[CHUNK_NOT_PRESENT])?,
                        Some(c) => {
                            out.write_all(&[CHUNK_PRESENT])?;
                            // `BitSet.toByteArray()` drops trailing zero bytes, then the region pads them back.
                            let bytes = c.bytes();
                            let len = bytes.len().min(expected);
                            out.write_all(&bytes[..len])?;
                            out.write_all(&vec![0u8; expected - len])?;
                        }
                    }
                }
            }
            for x in 0..32 {
                for z in 0..32 {
                    if let Some(c) = &self.chunks[Self::slot(x, z)] {
                        for name in c.overview() {
                            write_utf(&mut out, chunk::block_to_string(name))?;
                        }
                    }
                }
            }
            for x in 0..32 {
                for z in 0..32 {
                    if let Some(c) = &self.chunks[Self::slot(x, z)] {
                        let locs = c.relative_blocks();
                        out.write_all(&(locs.len() as u16).to_be_bytes())?;
                        for (name, positions) in locs {
                            write_utf(&mut out, chunk::block_to_string(name))?;
                            out.write_all(&(positions.len() as u16).to_be_bytes())?;
                            for p in positions {
                                out.write_all(&[(p.z << 4 | p.x) as u8])?;
                                out.write_all(&(p.y - self.dimension.min_y).to_be_bytes())?;
                            }
                        }
                    }
                }
            }
            for x in 0..32 {
                for z in 0..32 {
                    if let Some(c) = &self.chunks[Self::slot(x, z)] {
                        out.write_all(&c.cache_timestamp.to_be_bytes())?;
                    }
                }
            }
            out.finish()?.flush()?;
        }
        std::fs::rename(&tmp, &path)
    }

    /// Reads region `x, z` from `dir` (`load`): `Ok(None)` when there is no file.
    pub fn load(
        dir: &Path,
        x: i32,
        z: i32,
        dimension: Arc<Dimension>,
        table: &BlockTable,
    ) -> io::Result<Option<CachedRegion>> {
        let path = Self::file(dir, x, z);
        let file = match std::fs::File::open(&path) {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        let mut input =
            flate2::read::GzDecoder::new(io::BufReader::with_capacity(32768, file));
        let magic = read_i32(&mut input)?;
        if magic != CACHED_REGION_MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Bad magic value {magic}"),
            ));
        }
        let height = dimension.height;
        let size = chunk::size_in_bytes(chunk::size(height));
        let mut bits: Vec<Option<Box<[u8]>>> = vec![None; 32 * 32];
        for cx in 0..32 {
            for cz in 0..32 {
                match read_u8(&mut input)? {
                    CHUNK_PRESENT => {
                        let mut bytes = vec![0u8; size];
                        input.read_exact(&mut bytes)?;
                        bits[Self::slot(cx, cz)] = Some(bytes.into_boxed_slice());
                    }
                    CHUNK_NOT_PRESENT => {}
                    _ => {
                        return Err(io::Error::new(io::ErrorKind::InvalidData, "Malformed stream"));
                    }
                }
            }
        }
        let mut overviews: Vec<Option<Vec<Arc<str>>>> = vec![None; 32 * 32];
        for cx in 0..32 {
            for cz in 0..32 {
                if bits[Self::slot(cx, cz)].is_some() {
                    let mut ov = Vec::with_capacity(256);
                    for _ in 0..256 {
                        ov.push(Arc::from(chunk::string_to_block(&read_utf(&mut input)?)));
                    }
                    overviews[Self::slot(cx, cz)] = Some(ov);
                }
            }
        }
        let mut specials: Vec<Vec<(Arc<str>, Vec<BlockPos>)>> = vec![Vec::new(); 32 * 32];
        for cx in 0..32 {
            for cz in 0..32 {
                if bits[Self::slot(cx, cz)].is_some() {
                    let kinds = read_u16(&mut input)? as usize;
                    for _ in 0..kinds {
                        let name: Arc<str> = Arc::from(chunk::string_to_block(&read_utf(&mut input)?));
                        let mut count = read_u16(&mut input)? as usize;
                        if count == 0 {
                            // A chunk with 65536 of one tracked block (an all-air chunk can do this in the End).
                            count = 65536;
                        }
                        let mut locs = Vec::with_capacity(count.min(4096));
                        for _ in 0..count {
                            let xz = read_u8(&mut input)?;
                            let y = read_i32(&mut input)?;
                            locs.push(BlockPos::new(
                                (xz & 0x0f) as i32,
                                y + dimension.min_y,
                                ((xz >> 4) & 0x0f) as i32,
                            ));
                        }
                        specials[Self::slot(cx, cz)].push((name, locs));
                    }
                }
            }
        }
        let mut region = CachedRegion::empty(x, z, dimension.clone());
        let mut timestamps = vec![0i64; 32 * 32];
        for cx in 0..32 {
            for cz in 0..32 {
                if bits[Self::slot(cx, cz)].is_some() {
                    timestamps[Self::slot(cx, cz)] = read_i64(&mut input)?;
                }
            }
        }
        // Only a file read to the end without error sets any chunk.
        for cx in 0..32 {
            for cz in 0..32 {
                let i = Self::slot(cx, cz);
                if let (Some(data), Some(ov)) = (bits[i].take(), overviews[i].take()) {
                    region.chunks[i] = Some(Arc::new(CachedChunk::new(
                        cx + 32 * x,
                        cz + 32 * z,
                        height,
                        data,
                        ov.into_boxed_slice(),
                        std::mem::take(&mut specials[i]),
                        timestamps[i],
                        table,
                        dimension.min_y,
                    )));
                }
            }
        }
        Ok(Some(region))
    }
}

fn read_u8(r: &mut impl Read) -> io::Result<u8> {
    let mut b = [0u8; 1];
    r.read_exact(&mut b)?;
    Ok(b[0])
}

fn read_u16(r: &mut impl Read) -> io::Result<u16> {
    let mut b = [0u8; 2];
    r.read_exact(&mut b)?;
    Ok(u16::from_be_bytes(b))
}

fn read_i32(r: &mut impl Read) -> io::Result<i32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(i32::from_be_bytes(b))
}

fn read_i64(r: &mut impl Read) -> io::Result<i64> {
    let mut b = [0u8; 8];
    r.read_exact(&mut b)?;
    Ok(i64::from_be_bytes(b))
}

/// `DataInputStream.readUTF` (block ids are ASCII, where modified UTF-8 is plain UTF-8).
fn read_utf(r: &mut impl Read) -> io::Result<String> {
    let len = read_u16(r)? as usize;
    let mut b = vec![0u8; len];
    r.read_exact(&mut b)?;
    String::from_utf8(b).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// `DataOutputStream.writeUTF`.
fn write_utf(w: &mut impl Write, s: &str) -> io::Result<()> {
    let bytes = s.as_bytes();
    let len = u16::try_from(bytes.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "string too long"))?;
    w.write_all(&len.to_be_bytes())?;
    w.write_all(bytes)
}
