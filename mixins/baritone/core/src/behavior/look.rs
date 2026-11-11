//! `LookBehavior`: where Baritone turns the player, how (visibly, or only for the server with `freeLook`), and
//! the aim processor that turns a wanted rotation into one the mouse could produce (`randomLooking`).

use crate::math::Rotation;
use crate::settings::Settings;
use std::collections::VecDeque;

/// `ForkableRandom`: xoshiro256++ seeded with splitmix64.
#[derive(Clone, Debug)]
pub struct ForkableRandom {
    s: [u64; 4],
}

impl ForkableRandom {
    pub fn new(seed: u64) -> Self {
        let mut seed = seed;
        let mut splitmix = || {
            seed = seed.wrapping_add(0x9e3779b97f4a7c15);
            let mut z = seed;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
            z ^ (z >> 31)
        };
        Self {
            s: [splitmix(), splitmix(), splitmix(), splitmix()],
        }
    }

    pub fn from_time() -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x1234_5678);
        Self::new(now)
    }

    pub fn next_u64(&mut self) -> u64 {
        let s = &mut self.s;
        let result = s[0].wrapping_add(s[3]).rotate_left(23).wrapping_add(s[0]);
        let t = s[1] << 17;
        s[2] ^= s[0];
        s[3] ^= s[1];
        s[1] ^= s[2];
        s[0] ^= s[3];
        s[2] ^= t;
        s[3] = s[3].rotate_left(45);
        result
    }

    pub fn next_double(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    pub fn fork(&self) -> Self {
        self.clone()
    }
}

/// `AimProcessor`: the random offsets of the current tick and the mouse's resolution.
#[derive(Clone, Debug)]
pub struct AimProcessor {
    rand: ForkableRandom,
    random_yaw_offset: f64,
    random_pitch_offset: f64,
    /// The rotation `peek_rotation` starts from (`getPrevRotation`): the player's, or a fork's own.
    prev: Option<Rotation>,
}

impl Default for AimProcessor {
    fn default() -> Self {
        Self {
            rand: ForkableRandom::from_time(),
            random_yaw_offset: 0.0,
            random_pitch_offset: 0.0,
            prev: None,
        }
    }
}

impl AimProcessor {
    /// `peekRotation`: the rotation the player would end up at turning toward `rotation` from `current`.
    pub fn peek_rotation(&self, rotation: Rotation, current: Rotation, sensitivity: f64) -> Rotation {
        let prev = self.prev.unwrap_or(current);
        let mut desired_yaw = rotation.yaw;
        let mut desired_pitch = rotation.pitch;
        // A target that didn't care about pitch kept the player's: bring it toward level.
        if desired_pitch == prev.pitch {
            desired_pitch = nudge_to_level(desired_pitch);
        }
        desired_yaw += self.random_yaw_offset as f32;
        desired_pitch += self.random_pitch_offset as f32;
        Rotation::new(
            calculate_mouse_move(prev.yaw, desired_yaw, sensitivity),
            calculate_mouse_move(prev.pitch, desired_pitch, sensitivity),
        )
        .clamp()
    }

    /// `tick`: new random offsets (`randomLooking`, `randomLooking113`).
    pub fn tick(&mut self, settings: &Settings) {
        self.random_yaw_offset = (self.rand.next_double() - 0.5) * settings.random_looking;
        self.random_pitch_offset = (self.rand.next_double() - 0.5) * settings.random_looking;
        let mut random = self.rand.next_double() - 0.5;
        if random.abs() < 0.1 {
            random *= 4.0;
        }
        self.random_yaw_offset += random * settings.random_looking113;
    }

    /// `fork`: a copy that remembers its own last rotation, for simulating several ticks ahead.
    pub fn fork(&self, current: Rotation) -> AimProcessor {
        AimProcessor {
            rand: self.rand.fork(),
            random_yaw_offset: self.random_yaw_offset,
            random_pitch_offset: self.random_pitch_offset,
            prev: Some(self.prev.unwrap_or(current)),
        }
    }

    /// `nextRotation` of a fork: peek, then tick.
    pub fn next_rotation(&mut self, rotation: Rotation, current: Rotation, settings: &Settings, sensitivity: f64) -> Rotation {
        let actual = self.peek_rotation(rotation, current, sensitivity);
        self.tick(settings);
        if self.prev.is_some() {
            self.prev = Some(actual);
        }
        actual
    }
}

fn nudge_to_level(pitch: f32) -> f32 {
    if pitch < -20.0 {
        pitch + 1.0
    } else if pitch > 10.0 {
        pitch - 1.0
    } else {
        pitch
    }
}

fn calculate_mouse_move(current: f32, target: f32, sensitivity: f64) -> f32 {
    let delta = target - current;
    let delta_px = angle_to_mouse(delta, sensitivity);
    current + mouse_to_angle(delta_px, sensitivity)
}

fn angle_to_mouse(angle_delta: f32, sensitivity: f64) -> f64 {
    let min_angle_change = mouse_to_angle(1.0, sensitivity);
    (angle_delta / min_angle_change).round() as f64
}

fn mouse_to_angle(mouse_delta: f64, sensitivity: f64) -> f32 {
    // Minecraft's own mix of double and float scaling.
    let f = sensitivity * 0.6f32 as f64 + 0.2f32 as f64;
    (mouse_delta * f * f * f * 8.0) as f32 * 0.15
}

/// How a target rotation is applied (`LookBehavior.Target.Mode`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LookMode {
    /// Turn the player's view.
    Client,
    /// Turn the player for this tick only (movement, the server, the crosshair), then put the view back.
    Server,
    /// Don't turn at all.
    None,
}

impl LookMode {
    /// `Mode.resolve`.
    pub fn resolve(settings: &Settings, fall_flying: bool, block_interact: bool) -> LookMode {
        if fall_flying {
            return if settings.elytra_free_look {
                LookMode::Server
            } else {
                LookMode::Client
            };
        }
        if settings.free_look {
            if block_interact {
                return if settings.block_free_look {
                    LookMode::Server
                } else {
                    LookMode::Client
                };
            }
            return if settings.anti_cheat_compatibility {
                LookMode::Server
            } else {
                LookMode::None
            };
        }
        LookMode::Client
    }
}

/// The rotation Baritone wants this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LookTarget {
    pub rotation: Rotation,
    pub mode: LookMode,
}

/// What the game should do with the player's rotation around its tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LookCommand {
    /// The rotation to give the player before it moves this tick.
    pub rotation: Rotation,
    /// Put the view back to where it was after the tick (`Mode.SERVER`).
    pub restore_after: bool,
    /// After the tick, set the view to this instead (`smoothLook`).
    pub smoothed: Option<Rotation>,
}

/// `LookBehavior`.
#[derive(Debug, Default)]
pub struct LookBehavior {
    target: Option<LookTarget>,
    processor: AimProcessor,
    smooth_yaw: VecDeque<f32>,
    smooth_pitch: VecDeque<f32>,
}

impl LookBehavior {
    /// `updateTarget`.
    pub fn update_target(&mut self, rotation: Rotation, block_interact: bool, settings: &Settings, fall_flying: bool) {
        self.target = Some(LookTarget {
            rotation,
            mode: LookMode::resolve(settings, fall_flying, block_interact),
        });
    }

    pub fn target(&self) -> Option<LookTarget> {
        self.target
    }

    pub fn aim(&self) -> &AimProcessor {
        &self.processor
    }

    /// `onTick` (IN): new random offsets.
    pub fn tick(&mut self, settings: &Settings) {
        self.processor.tick(settings);
    }

    /// `peekRotation` from the player's current rotation.
    pub fn peek(&self, rotation: Rotation, current: Rotation, sensitivity: f64) -> Rotation {
        self.processor.peek_rotation(rotation, current, sensitivity)
    }

    /// `onPlayerUpdate` PRE and POST at once: what the game should do with this tick's target, which is then
    /// used up (`target = null` on POST).
    pub fn take_command(
        &mut self,
        current: Rotation,
        settings: &Settings,
        sensitivity: f64,
        fall_flying: bool,
    ) -> Option<LookCommand> {
        let target = self.target.take()?;
        if target.mode == LookMode::None {
            return None;
        }
        let actual = self.processor.peek_rotation(target.rotation, current, sensitivity);
        let ticks = settings.smooth_look_ticks.max(0) as usize;
        self.smooth_yaw.push_back(target.rotation.yaw);
        while self.smooth_yaw.len() > ticks {
            self.smooth_yaw.pop_front();
        }
        self.smooth_pitch.push_back(target.rotation.pitch);
        while self.smooth_pitch.len() > ticks {
            self.smooth_pitch.pop_front();
        }
        let restore_after = target.mode == LookMode::Server;
        let smooth = if fall_flying {
            settings.elytra_smooth_look
        } else {
            settings.smooth_look
        };
        let smoothed = if !restore_after && smooth {
            let avg = |b: &VecDeque<f32>, d: f32| {
                if b.is_empty() {
                    d
                } else {
                    (b.iter().map(|v| *v as f64).sum::<f64>() / b.len() as f64) as f32
                }
            };
            Some(Rotation::new(
                avg(&self.smooth_yaw, current.yaw),
                if fall_flying {
                    avg(&self.smooth_pitch, current.pitch)
                } else {
                    actual.pitch
                },
            ))
        } else {
            None
        };
        Some(LookCommand {
            rotation: actual,
            restore_after,
            smoothed,
        })
    }

    /// `onWorldEvent`.
    pub fn on_world_change(&mut self) {
        self.target = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mouse_resolution_rounds_turns() {
        let s = Settings::default();
        let mut aim = AimProcessor::default();
        aim.tick(&Settings {
            random_looking: 0.0,
            random_looking113: 0.0,
            ..s.clone()
        });
        let r = aim.peek_rotation(Rotation::new(90.0, 0.5), Rotation::new(0.0, 0.0), 0.5);
        // Within one mouse step (0.15 * (0.5*0.6+0.2)^3 * 8 = 0.15 degrees) of the target.
        assert!((r.yaw - 90.0).abs() < 0.16, "{r:?}");
        assert!((r.pitch - 0.5).abs() < 0.16, "{r:?}");
    }

    #[test]
    fn free_look_modes() {
        let s = Settings::default();
        assert_eq!(LookMode::resolve(&s, false, false), LookMode::Server);
        assert_eq!(LookMode::resolve(&s, false, true), LookMode::Client);
        let s = Settings {
            free_look: false,
            ..s
        };
        assert_eq!(LookMode::resolve(&s, false, false), LookMode::Client);
    }
}
