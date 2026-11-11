//! Positions, vectors, directions, boxes and rotations, with Minecraft's own `Mth` arithmetic where Presto
//! relied on it (the sine table, `floor`, `atan2`), so aim and ray casts agree with the game's.

use std::fmt;
use std::ops::{Add, Mul, Neg, Sub};
use std::sync::OnceLock;

/// `net.minecraft.util.Mth`.
pub mod mth {
    use super::OnceLock;

    static SIN: OnceLock<Box<[f32]>> = OnceLock::new();

    fn table() -> &'static [f32] {
        SIN.get_or_init(|| {
            (0..65536)
                .map(|i| (i as f64 * std::f64::consts::PI * 2.0 / 65536.0).sin() as f32)
                .collect()
        })
    }

    /// `Mth.sin`: the game's 65536-entry sine table.
    pub fn sin(value: f32) -> f32 {
        table()[((value * 10430.378) as i64 as i32 & 65535) as usize]
    }

    /// `Mth.cos`.
    pub fn cos(value: f32) -> f32 {
        table()[((value * 10430.378 + 16384.0) as i64 as i32 & 65535) as usize]
    }

    pub fn floor(value: f64) -> i32 {
        value.floor() as i32
    }

    pub fn wrap_degrees(value: f32) -> f32 {
        let mut f = value % 360.0;
        if f >= 180.0 {
            f -= 360.0;
        }
        if f < -180.0 {
            f += 360.0;
        }
        f
    }

    pub fn wrap_degrees_f64(value: f64) -> f64 {
        let mut f = value % 360.0;
        if f >= 180.0 {
            f -= 360.0;
        }
        if f < -180.0 {
            f += 360.0;
        }
        f
    }

    /// `Mth.atan2`: the game's table-based arc tangent.
    pub fn atan2(mut y: f64, mut x: f64) -> f64 {
        static TABLES: OnceLock<(Box<[f64]>, Box<[f64]>)> = OnceLock::new();
        let (asin_tab, cos_tab) = TABLES.get_or_init(|| {
            let mut asin = vec![0.0; 257];
            let mut cos = vec![0.0; 257];
            for i in 0..257 {
                let d0 = i as f64 / 256.0;
                let d1 = d0.asin();
                cos[i] = d1.cos();
                asin[i] = d1;
            }
            (asin.into_boxed_slice(), cos.into_boxed_slice())
        });
        let frac_bias = f64::from_bits(4805340802404319232);
        let d0 = x * x + y * y;
        if d0.is_nan() {
            return f64::NAN;
        }
        let flag = y < 0.0;
        if flag {
            y = -y;
        }
        let flag1 = x < 0.0;
        if flag1 {
            x = -x;
        }
        let flag2 = y > x;
        if flag2 {
            std::mem::swap(&mut x, &mut y);
        }
        let d9 = fast_inv_sqrt(d0);
        x *= d9;
        y *= d9;
        let d2 = frac_bias + y;
        let i = d2.to_bits() as i32 as usize;
        let d3 = asin_tab[i];
        let d4 = cos_tab[i];
        let d5 = d2 - frac_bias;
        let d6 = y * d4 - x * d5;
        let d7 = (6.0 + d6 * d6) * d6 * 0.16666666666666666;
        let mut d8 = d3 + d7;
        if flag2 {
            d8 = std::f64::consts::FRAC_PI_2 - d8;
        }
        if flag1 {
            d8 = std::f64::consts::PI - d8;
        }
        if flag {
            d8 = -d8;
        }
        d8
    }

    fn fast_inv_sqrt(x: f64) -> f64 {
        let d0 = 0.5 * x;
        let i = 6910469410427058090_i64 - ((x.to_bits() as i64) >> 1);
        let x = f64::from_bits(i as u64);
        x * (1.5 - d0 * x * x)
    }

    pub fn clamp(v: f64, lo: f64, hi: f64) -> f64 {
        v.max(lo).min(hi)
    }
}

/// A block position (`BetterBlockPos`): Baritone's own hash, and `serializeToLong`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct BlockPos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl std::hash::Hash for BlockPos {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_i64(Self::long_hash(self.x, self.y, self.z));
    }
}

impl fmt::Debug for BlockPos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({}, {}, {})", self.x, self.y, self.z)
    }
}

impl fmt::Display for BlockPos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} {}", self.x, self.y, self.z)
    }
}

impl BlockPos {
    pub const ORIGIN: BlockPos = BlockPos { x: 0, y: 0, z: 0 };

    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    /// The block holding a point (`BetterBlockPos(double, double, double)`).
    pub fn of(x: f64, y: f64, z: f64) -> Self {
        Self::new(mth::floor(x), mth::floor(y), mth::floor(z))
    }

    pub fn of_vec(v: Vec3) -> Self {
        Self::of(v.x, v.y, v.z)
    }

    /// `BetterBlockPos.longHash`.
    #[inline]
    pub const fn long_hash(x: i32, y: i32, z: i32) -> i64 {
        let mut hash: i64 = 3241;
        hash = 3457689_i64.wrapping_mul(hash).wrapping_add(x as i64);
        hash = 8734625_i64.wrapping_mul(hash).wrapping_add(y as i64);
        hash = 2873465_i64.wrapping_mul(hash).wrapping_add(z as i64);
        hash
    }

    #[inline]
    pub const fn hash_long(self) -> i64 {
        Self::long_hash(self.x, self.y, self.z)
    }

    /// `BetterBlockPos.serializeToLong`: 26 bits of x and z, 12 of y.
    pub const fn to_long(self) -> i64 {
        const X_MASK: i64 = (1 << 26) - 1;
        const Y_MASK: i64 = (1 << 12) - 1;
        const Z_MASK: i64 = (1 << 26) - 1;
        ((self.x as i64) & X_MASK) << 38 | ((self.y as i64) & Y_MASK) << 26 | ((self.z as i64) & Z_MASK)
    }

    pub const fn from_long(serialized: i64) -> Self {
        let x = (serialized >> 38) as i32;
        let y = ((serialized << 26) >> 52) as i32;
        let z = ((serialized << 38) >> 38) as i32;
        Self { x, y, z }
    }

    #[inline]
    pub const fn above(self) -> Self {
        Self::new(self.x, self.y + 1, self.z)
    }
    #[inline]
    pub const fn above_n(self, n: i32) -> Self {
        Self::new(self.x, self.y + n, self.z)
    }
    #[inline]
    pub const fn below(self) -> Self {
        Self::new(self.x, self.y - 1, self.z)
    }
    #[inline]
    pub const fn below_n(self, n: i32) -> Self {
        Self::new(self.x, self.y - n, self.z)
    }
    pub const fn north(self) -> Self {
        Self::new(self.x, self.y, self.z - 1)
    }
    pub const fn south(self) -> Self {
        Self::new(self.x, self.y, self.z + 1)
    }
    pub const fn east(self) -> Self {
        Self::new(self.x + 1, self.y, self.z)
    }
    pub const fn west(self) -> Self {
        Self::new(self.x - 1, self.y, self.z)
    }
    #[inline]
    pub const fn offset(self, dx: i32, dy: i32, dz: i32) -> Self {
        Self::new(self.x + dx, self.y + dy, self.z + dz)
    }
    #[inline]
    pub fn relative(self, dir: Direction) -> Self {
        let [dx, dy, dz] = dir.normal();
        self.offset(dx, dy, dz)
    }
    pub fn relative_n(self, dir: Direction, n: i32) -> Self {
        let [dx, dy, dz] = dir.normal();
        self.offset(dx * n, dy * n, dz * n)
    }
    pub fn subtract(self, other: BlockPos) -> BlockPos {
        Self::new(self.x - other.x, self.y - other.y, self.z - other.z)
    }
    pub fn add(self, other: BlockPos) -> BlockPos {
        Self::new(self.x + other.x, self.y + other.y, self.z + other.z)
    }
    pub fn dist_sqr(self, to: BlockPos) -> f64 {
        let dx = self.x as f64 - to.x as f64;
        let dy = self.y as f64 - to.y as f64;
        let dz = self.z as f64 - to.z as f64;
        dx * dx + dy * dy + dz * dz
    }
    pub fn distance_to(self, to: BlockPos) -> f64 {
        self.dist_sqr(to).sqrt()
    }
    /// `Vec3i.distSqr` to a point, from this block's corner.
    pub fn dist_sqr_point(self, x: f64, y: f64, z: f64) -> f64 {
        let dx = self.x as f64 - x;
        let dy = self.y as f64 - y;
        let dz = self.z as f64 - z;
        dx * dx + dy * dy + dz * dz
    }
    /// `Vec3i.distManhattan`.
    pub fn manhattan(self, to: BlockPos) -> i32 {
        (self.x - to.x).abs() + (self.y - to.y).abs() + (self.z - to.z).abs()
    }
    pub fn center(self) -> Vec3 {
        Vec3::new(self.x as f64 + 0.5, self.y as f64 + 0.5, self.z as f64 + 0.5)
    }
    pub fn corner(self) -> Vec3 {
        Vec3::new(self.x as f64, self.y as f64, self.z as f64)
    }
    /// The chunk column holding this block.
    pub const fn chunk(self) -> (i32, i32) {
        (self.x >> 4, self.z >> 4)
    }
}

impl From<[i32; 3]> for BlockPos {
    fn from(v: [i32; 3]) -> Self {
        Self::new(v[0], v[1], v[2])
    }
}

impl From<BlockPos> for [i32; 3] {
    fn from(p: BlockPos) -> Self {
        [p.x, p.y, p.z]
    }
}

/// `net.minecraft.core.Direction`, in its ordinal order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Direction {
    Down,
    Up,
    North,
    South,
    West,
    East,
}

impl Direction {
    pub const ALL: [Direction; 6] = [
        Direction::Down,
        Direction::Up,
        Direction::North,
        Direction::South,
        Direction::West,
        Direction::East,
    ];
    /// `Direction.Plane.HORIZONTAL`'s order (`Direction.from2DDataValue`).
    pub const HORIZONTAL: [Direction; 4] = [
        Direction::South,
        Direction::West,
        Direction::North,
        Direction::East,
    ];
    /// `Movement.HORIZONTALS_BUT_ALSO_DOWN_____SO_EVERY_DIRECTION_EXCEPT_UP`.
    pub const HORIZONTALS_AND_DOWN: [Direction; 5] = [
        Direction::North,
        Direction::South,
        Direction::East,
        Direction::West,
        Direction::Down,
    ];

    pub const fn normal(self) -> [i32; 3] {
        match self {
            Direction::Down => [0, -1, 0],
            Direction::Up => [0, 1, 0],
            Direction::North => [0, 0, -1],
            Direction::South => [0, 0, 1],
            Direction::West => [-1, 0, 0],
            Direction::East => [1, 0, 0],
        }
    }
    pub const fn opposite(self) -> Direction {
        match self {
            Direction::Down => Direction::Up,
            Direction::Up => Direction::Down,
            Direction::North => Direction::South,
            Direction::South => Direction::North,
            Direction::West => Direction::East,
            Direction::East => Direction::West,
        }
    }
    pub const fn ordinal(self) -> usize {
        self as usize
    }
    pub fn from_ordinal(i: usize) -> Option<Direction> {
        Self::ALL.get(i).copied()
    }
    pub fn name(self) -> &'static str {
        match self {
            Direction::Down => "down",
            Direction::Up => "up",
            Direction::North => "north",
            Direction::South => "south",
            Direction::West => "west",
            Direction::East => "east",
        }
    }
    pub fn from_name(s: &str) -> Option<Direction> {
        Self::ALL
            .into_iter()
            .find(|d| d.name().eq_ignore_ascii_case(s))
    }
    pub const fn axis(self) -> Axis {
        match self {
            Direction::Down | Direction::Up => Axis::Y,
            Direction::North | Direction::South => Axis::Z,
            Direction::West | Direction::East => Axis::X,
        }
    }
    pub const fn is_horizontal(self) -> bool {
        !matches!(self, Direction::Down | Direction::Up)
    }
    /// The direction with this offset, if it is a unit vector.
    pub fn from_delta(dx: i32, dy: i32, dz: i32) -> Option<Direction> {
        Self::ALL.into_iter().find(|d| d.normal() == [dx, dy, dz])
    }
    /// `Direction.getClockWise` (around Y).
    pub fn clockwise(self) -> Direction {
        match self {
            Direction::North => Direction::East,
            Direction::East => Direction::South,
            Direction::South => Direction::West,
            Direction::West => Direction::North,
            d => d,
        }
    }
    pub fn counter_clockwise(self) -> Direction {
        match self {
            Direction::North => Direction::West,
            Direction::West => Direction::South,
            Direction::South => Direction::East,
            Direction::East => Direction::North,
            d => d,
        }
    }
    /// `Direction.fromYRot`: the horizontal direction an entity with this yaw faces.
    pub fn from_yaw(yaw: f64) -> Direction {
        let i = ((yaw / 90.0 + 0.5).floor() as i32) & 3;
        Self::HORIZONTAL[i as usize]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Axis {
    X,
    Y,
    Z,
}

/// `net.minecraft.world.phys.Vec3`.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3 {
    pub const ZERO: Vec3 = Vec3 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }
    pub fn length_sqr(self) -> f64 {
        self.x * self.x + self.y * self.y + self.z * self.z
    }
    pub fn length(self) -> f64 {
        self.length_sqr().sqrt()
    }
    pub fn horizontal_length_sqr(self) -> f64 {
        self.x * self.x + self.z * self.z
    }
    pub fn distance_to_sqr(self, o: Vec3) -> f64 {
        (self - o).length_sqr()
    }
    pub fn distance_to(self, o: Vec3) -> f64 {
        self.distance_to_sqr(o).sqrt()
    }
    pub fn normalize(self) -> Vec3 {
        let l = self.length();
        if l < 1.0e-4 {
            Vec3::ZERO
        } else {
            Vec3::new(self.x / l, self.y / l, self.z / l)
        }
    }
    pub fn dot(self, o: Vec3) -> f64 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    pub fn scale(self, s: f64) -> Vec3 {
        Vec3::new(self.x * s, self.y * s, self.z * s)
    }
    pub fn with_y(self, y: f64) -> Vec3 {
        Vec3::new(self.x, y, self.z)
    }
    pub fn to_array(self) -> [f64; 3] {
        [self.x, self.y, self.z]
    }
}

impl From<[f64; 3]> for Vec3 {
    fn from(v: [f64; 3]) -> Self {
        Self::new(v[0], v[1], v[2])
    }
}

impl Add for Vec3 {
    type Output = Vec3;
    fn add(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}

impl Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}

impl Mul<f64> for Vec3 {
    type Output = Vec3;
    fn mul(self, s: f64) -> Vec3 {
        self.scale(s)
    }
}

impl Neg for Vec3 {
    type Output = Vec3;
    fn neg(self) -> Vec3 {
        Vec3::new(-self.x, -self.y, -self.z)
    }
}

/// `net.minecraft.world.phys.AABB`.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub fn new(x0: f64, y0: f64, z0: f64, x1: f64, y1: f64, z1: f64) -> Self {
        Self {
            min: Vec3::new(x0.min(x1), y0.min(y1), z0.min(z1)),
            max: Vec3::new(x0.max(x1), y0.max(y1), z0.max(z1)),
        }
    }
    pub fn from_array(b: [f64; 6]) -> Self {
        Self::new(b[0], b[1], b[2], b[3], b[4], b[5])
    }
    pub fn block(pos: BlockPos) -> Self {
        Self::new(
            pos.x as f64,
            pos.y as f64,
            pos.z as f64,
            pos.x as f64 + 1.0,
            pos.y as f64 + 1.0,
            pos.z as f64 + 1.0,
        )
    }
    pub fn moved(self, x: f64, y: f64, z: f64) -> Self {
        Self {
            min: Vec3::new(self.min.x + x, self.min.y + y, self.min.z + z),
            max: Vec3::new(self.max.x + x, self.max.y + y, self.max.z + z),
        }
    }
    pub fn inflate(self, x: f64, y: f64, z: f64) -> Self {
        Self::new(
            self.min.x - x,
            self.min.y - y,
            self.min.z - z,
            self.max.x + x,
            self.max.y + y,
            self.max.z + z,
        )
    }
    pub fn intersects(self, o: Aabb) -> bool {
        self.min.x < o.max.x
            && self.max.x > o.min.x
            && self.min.y < o.max.y
            && self.max.y > o.min.y
            && self.min.z < o.max.z
            && self.max.z > o.min.z
    }
    pub fn contains(self, p: Vec3) -> bool {
        p.x >= self.min.x
            && p.x < self.max.x
            && p.y >= self.min.y
            && p.y < self.max.y
            && p.z >= self.min.z
            && p.z < self.max.z
    }
    pub fn center(self) -> Vec3 {
        Vec3::new(
            (self.min.x + self.max.x) / 2.0,
            (self.min.y + self.max.y) / 2.0,
            (self.min.z + self.max.z) / 2.0,
        )
    }
    /// `AABB.clip`: where a segment enters the box, and through which face.
    pub fn clip(self, from: Vec3, to: Vec3) -> Option<(Vec3, Direction)> {
        let d = to - from;
        let mut t_hit = 1.0f64;
        let mut face = None;
        let mut check = |t: f64, dir: Direction, inside: &dyn Fn(Vec3) -> bool| {
            if t > -1.0e-7 && t < t_hit {
                let p = from + d * t;
                if inside(p) {
                    t_hit = t;
                    face = Some(dir);
                }
            }
        };
        let eps = 1.0e-7;
        if d.x > eps {
            check(
                (self.min.x - from.x) / d.x,
                Direction::West,
                &|p| self.within_yz(p),
            );
        } else if d.x < -eps {
            check(
                (self.max.x - from.x) / d.x,
                Direction::East,
                &|p| self.within_yz(p),
            );
        }
        if d.y > eps {
            check(
                (self.min.y - from.y) / d.y,
                Direction::Down,
                &|p| self.within_xz(p),
            );
        } else if d.y < -eps {
            check(
                (self.max.y - from.y) / d.y,
                Direction::Up,
                &|p| self.within_xz(p),
            );
        }
        if d.z > eps {
            check(
                (self.min.z - from.z) / d.z,
                Direction::North,
                &|p| self.within_xy(p),
            );
        } else if d.z < -eps {
            check(
                (self.max.z - from.z) / d.z,
                Direction::South,
                &|p| self.within_xy(p),
            );
        }
        face.map(|f| (from + d * t_hit, f))
    }
    fn within_yz(self, p: Vec3) -> bool {
        p.y >= self.min.y - 1.0e-7
            && p.y <= self.max.y + 1.0e-7
            && p.z >= self.min.z - 1.0e-7
            && p.z <= self.max.z + 1.0e-7
    }
    fn within_xz(self, p: Vec3) -> bool {
        p.x >= self.min.x - 1.0e-7
            && p.x <= self.max.x + 1.0e-7
            && p.z >= self.min.z - 1.0e-7
            && p.z <= self.max.z + 1.0e-7
    }
    fn within_xy(self, p: Vec3) -> bool {
        p.x >= self.min.x - 1.0e-7
            && p.x <= self.max.x + 1.0e-7
            && p.y >= self.min.y - 1.0e-7
            && p.y <= self.max.y + 1.0e-7
    }
}

/// A yaw and pitch, in degrees (`baritone.api.utils.Rotation`).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Rotation {
    pub yaw: f32,
    pub pitch: f32,
}

impl Rotation {
    pub const fn new(yaw: f32, pitch: f32) -> Self {
        Self { yaw, pitch }
    }
    pub fn is_valid(self) -> bool {
        self.yaw.is_finite() && self.pitch.is_finite()
    }
    pub fn add(self, o: Rotation) -> Rotation {
        Rotation::new(self.yaw + o.yaw, self.pitch + o.pitch)
    }
    pub fn subtract(self, o: Rotation) -> Rotation {
        Rotation::new(self.yaw - o.yaw, self.pitch - o.pitch)
    }
    pub fn clamp(self) -> Rotation {
        Rotation::new(self.yaw, Self::clamp_pitch(self.pitch))
    }
    pub fn normalize(self) -> Rotation {
        Rotation::new(Self::normalize_yaw(self.yaw), self.pitch)
    }
    pub fn normalize_and_clamp(self) -> Rotation {
        Rotation::new(
            Self::normalize_yaw(self.yaw),
            Self::clamp_pitch(self.pitch),
        )
    }
    pub fn with_pitch(self, pitch: f32) -> Rotation {
        Rotation::new(self.yaw, pitch)
    }
    pub fn is_really_close_to(self, o: Rotation) -> bool {
        self.yaw_is_really_close(o) && (self.pitch - o.pitch).abs() < 0.01
    }
    pub fn yaw_is_really_close(self, o: Rotation) -> bool {
        let diff = (Self::normalize_yaw(self.yaw) - Self::normalize_yaw(o.yaw)).abs();
        !(0.01..=359.99).contains(&diff)
    }
    pub fn clamp_pitch(pitch: f32) -> f32 {
        pitch.clamp(-90.0, 90.0)
    }
    /// `Rotation.normalizeYaw`: Java's `%` keeps the sign, as Rust's does.
    pub fn normalize_yaw(yaw: f32) -> f32 {
        let mut y = yaw % 360.0;
        if y < -180.0 {
            y += 360.0;
        }
        if y > 180.0 {
            y -= 360.0;
        }
        y
    }
    pub fn yaw_distance_from_offset(yaw: f32, offset_yaw: f32) -> f32 {
        if ((yaw > 0.0) ^ (offset_yaw > 0.0))
            && ((!(-90.0..=90.0).contains(&yaw)) ^ (!(-90.0..=90.0).contains(&offset_yaw)))
        {
            if yaw < 0.0 {
                360.0 + (yaw - offset_yaw)
            } else {
                360.0 - (yaw - offset_yaw)
            }
        } else {
            yaw - offset_yaw
        }
    }
}

impl fmt::Display for Rotation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Yaw: {}, Pitch: {}", self.yaw, self.pitch)
    }
}

/// `RotationUtils`: the rotation math that doesn't touch the world (`world::raytrace` has the rest).
pub mod rotation_utils {
    use super::*;

    pub const DEG_TO_RAD: f64 = std::f64::consts::PI / 180.0;
    pub const DEG_TO_RAD_F: f32 = DEG_TO_RAD as f32;
    pub const RAD_TO_DEG: f64 = 180.0 / std::f64::consts::PI;

    /// `wrapAnglesToRelative`: the target expressed as the shortest turn from `current`.
    pub fn wrap_angles_to_relative(current: Rotation, target: Rotation) -> Rotation {
        if current.yaw_is_really_close(target) {
            return Rotation::new(current.yaw, target.pitch);
        }
        target.subtract(current).normalize().add(current)
    }

    pub fn calc_rotation_from_vec3d_rel(orig: Vec3, dest: Vec3, current: Rotation) -> Rotation {
        wrap_angles_to_relative(current, calc_rotation_from_vec3d(orig, dest))
    }

    pub fn calc_rotation_from_vec3d(orig: Vec3, dest: Vec3) -> Rotation {
        let delta = [orig.x - dest.x, orig.y - dest.y, orig.z - dest.z];
        let yaw = mth::atan2(delta[0], -delta[2]);
        let dist = (delta[0] * delta[0] + delta[2] * delta[2]).sqrt();
        let pitch = mth::atan2(delta[1], dist);
        Rotation::new((yaw * RAD_TO_DEG) as f32, (pitch * RAD_TO_DEG) as f32)
    }

    pub fn calc_rotation_from_coords(orig: BlockPos, dest: BlockPos) -> Rotation {
        calc_rotation_from_vec3d(orig.corner(), dest.corner())
    }

    /// `calcLookDirectionFromRotation`.
    pub fn look_direction(rotation: Rotation) -> Vec3 {
        let flat_z = mth::cos((-rotation.yaw * DEG_TO_RAD_F) - std::f32::consts::PI);
        let flat_x = mth::sin((-rotation.yaw * DEG_TO_RAD_F) - std::f32::consts::PI);
        let pitch_base = -mth::cos(-rotation.pitch * DEG_TO_RAD_F);
        let pitch_height = mth::sin(-rotation.pitch * DEG_TO_RAD_F);
        Vec3::new(
            (flat_x * pitch_base) as f64,
            pitch_height as f64,
            (flat_z * pitch_base) as f64,
        )
    }
}

/// `VecUtils.distanceToCenter`.
pub fn distance_to_center(pos: BlockPos, x: f64, y: f64, z: f64) -> f64 {
    let dx = pos.x as f64 + 0.5 - x;
    let dy = pos.y as f64 + 0.5 - y;
    let dz = pos.z as f64 + 0.5 - z;
    (dx * dx + dy * dy + dz * dz).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_round_trip() {
        for p in [
            BlockPos::new(0, 0, 0),
            BlockPos::new(-1, -64, 1),
            BlockPos::new(30_000_000, 319, -30_000_000),
            BlockPos::new(-123456, 2047, 654321),
        ] {
            assert_eq!(BlockPos::from_long(p.to_long()), p);
        }
    }

    #[test]
    fn long_hash_matches_java() {
        // new BetterBlockPos(1, 2, 3).hashCode() == (int) longHash(1, 2, 3)
        // Java's long arithmetic wraps on overflow.
        let h = BlockPos::long_hash(1, 2, 3);
        let mut j: i64 = 3241;
        j = 3457689_i64.wrapping_mul(j).wrapping_add(1);
        j = 8734625_i64.wrapping_mul(j).wrapping_add(2);
        j = 2873465_i64.wrapping_mul(j).wrapping_add(3);
        assert_eq!(h, j);
    }

    #[test]
    fn mth_trig_matches_std_closely() {
        for i in -100..100 {
            let a = i as f32 * 0.1;
            assert!((mth::sin(a) - a.sin()).abs() < 1e-3);
            assert!((mth::cos(a) - a.cos()).abs() < 1e-3);
        }
        for (y, x) in [(1.0, 1.0), (-1.0, 0.5), (0.3, -2.0), (-5.0, -5.0), (0.0, 1.0)] {
            assert!((mth::atan2(y, x) - f64::atan2(y, x)).abs() < 1e-6, "{y} {x}");
        }
    }

    #[test]
    fn rotations_face_the_right_way() {
        // Facing +Z is yaw 0; -X is yaw 90; looking straight down is pitch 90.
        let r = rotation_utils::calc_rotation_from_vec3d(Vec3::ZERO, Vec3::new(0.0, 0.0, 1.0));
        assert!(r.yaw.abs() < 1e-3 && r.pitch.abs() < 1e-3);
        let r = rotation_utils::calc_rotation_from_vec3d(Vec3::ZERO, Vec3::new(-1.0, 0.0, 0.0));
        assert!((r.yaw - 90.0).abs() < 1e-3);
        let r = rotation_utils::calc_rotation_from_vec3d(Vec3::ZERO, Vec3::new(0.0, -1.0, 0.0));
        assert!((r.pitch - 90.0).abs() < 1e-3);
        let d = rotation_utils::look_direction(Rotation::new(90.0, 0.0));
        assert!((d.x + 1.0).abs() < 1e-3 && d.z.abs() < 1e-3);
    }

    #[test]
    fn aabb_clip_finds_entry_face() {
        let b = Aabb::new(0.0, 0.0, 0.0, 1.0, 1.0, 1.0);
        let (hit, face) = b
            .clip(Vec3::new(-1.0, 0.5, 0.5), Vec3::new(2.0, 0.5, 0.5))
            .unwrap();
        assert_eq!(face, Direction::West);
        assert!((hit.x - 0.0).abs() < 1e-9);
        let (_, face) = b
            .clip(Vec3::new(0.5, 3.0, 0.5), Vec3::new(0.5, -1.0, 0.5))
            .unwrap();
        assert_eq!(face, Direction::Up);
        assert!(
            b.clip(Vec3::new(-1.0, 2.0, 0.5), Vec3::new(2.0, 2.0, 0.5))
                .is_none()
        );
    }

    #[test]
    fn direction_from_yaw() {
        assert_eq!(Direction::from_yaw(0.0), Direction::South);
        assert_eq!(Direction::from_yaw(90.0), Direction::West);
        assert_eq!(Direction::from_yaw(180.0), Direction::North);
        assert_eq!(Direction::from_yaw(-90.0), Direction::East);
    }
}
