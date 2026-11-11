//! Waypoints (`Waypoint`, `WaypointCollection`): named positions by tag, saved per world and dimension in
//! Java Baritone's `<tag>.mp4` files.

use crate::math::BlockPos;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

const WAYPOINT_MAGIC_VALUE: i64 = 121977993584;

/// `IWaypoint.Tag`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Tag {
    Home,
    Death,
    Bed,
    User,
}

impl Tag {
    pub const ALL: [Tag; 4] = [Tag::Home, Tag::Death, Tag::Bed, Tag::User];

    pub fn names(self) -> &'static [&'static str] {
        match self {
            Tag::Home => &["home", "base"],
            Tag::Death => &["death"],
            Tag::Bed => &["bed", "spawn"],
            Tag::User => &["user"],
        }
    }
    pub fn name(self) -> &'static str {
        self.names()[0]
    }
    pub fn by_name(name: &str) -> Option<Tag> {
        Self::ALL
            .into_iter()
            .find(|t| t.names().iter().any(|n| n.eq_ignore_ascii_case(name)))
    }
    pub fn all_names() -> Vec<&'static str> {
        Self::ALL.iter().flat_map(|t| t.names().iter().copied()).collect()
    }
    fn file_name(self) -> &'static str {
        match self {
            Tag::Home => "home.mp4",
            Tag::Death => "death.mp4",
            Tag::Bed => "bed.mp4",
            Tag::User => "user.mp4",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Waypoint {
    pub name: String,
    pub tag: Tag,
    pub location: BlockPos,
    pub creation_timestamp: i64,
}

impl PartialEq for Waypoint {
    /// `Waypoint.equals`: name, tag and location (the time doesn't count).
    fn eq(&self, o: &Self) -> bool {
        self.name == o.name && self.tag == o.tag && self.location == o.location
    }
}

impl Waypoint {
    pub fn new(name: impl Into<String>, tag: Tag, location: BlockPos) -> Self {
        Self {
            name: name.into(),
            tag,
            location,
            creation_timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0),
        }
    }
}

/// `WaypointCollection`.
#[derive(Debug, Default)]
pub struct WaypointCollection {
    directory: Option<PathBuf>,
    waypoints: Vec<Waypoint>,
}

impl WaypointCollection {
    /// Waypoints kept in `directory`, read now.
    pub fn open(directory: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&directory);
        let mut me = Self {
            directory: Some(directory),
            waypoints: Vec::new(),
        };
        for tag in Tag::ALL {
            if let Some(dir) = &me.directory
                && let Ok(list) = Self::load(dir, tag)
            {
                me.waypoints.extend(list);
            }
        }
        me
    }

    /// Waypoints that are never saved.
    pub fn in_memory() -> Self {
        Self::default()
    }

    fn load(dir: &Path, tag: Tag) -> io::Result<Vec<Waypoint>> {
        let mut file = io::BufReader::new(std::fs::File::open(dir.join(tag.file_name()))?);
        let mut b8 = [0u8; 8];
        file.read_exact(&mut b8)?;
        if i64::from_be_bytes(b8) != WAYPOINT_MAGIC_VALUE {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Bad magic value"));
        }
        file.read_exact(&mut b8)?;
        let len = i64::from_be_bytes(b8).max(0);
        let mut out = Vec::new();
        for _ in 0..len {
            let mut b2 = [0u8; 2];
            file.read_exact(&mut b2)?;
            let mut name = vec![0u8; u16::from_be_bytes(b2) as usize];
            file.read_exact(&mut name)?;
            file.read_exact(&mut b8)?;
            let ts = i64::from_be_bytes(b8);
            let mut b4 = [0u8; 4];
            let mut coord = || -> io::Result<i32> {
                file.read_exact(&mut b4)?;
                Ok(i32::from_be_bytes(b4))
            };
            let (x, y, z) = (coord()?, coord()?, coord()?);
            out.push(Waypoint {
                name: String::from_utf8_lossy(&name).into_owned(),
                tag,
                location: BlockPos::new(x, y, z),
                creation_timestamp: ts,
            });
        }
        Ok(out)
    }

    fn save(&self, tag: Tag) {
        let Some(dir) = &self.directory else { return };
        let result = (|| -> io::Result<()> {
            let mut out = io::BufWriter::new(std::fs::File::create(dir.join(tag.file_name()))?);
            let list: Vec<&Waypoint> = self.waypoints.iter().filter(|w| w.tag == tag).collect();
            out.write_all(&WAYPOINT_MAGIC_VALUE.to_be_bytes())?;
            out.write_all(&(list.len() as i64).to_be_bytes())?;
            for w in list {
                let name = w.name.as_bytes();
                out.write_all(&(name.len().min(u16::MAX as usize) as u16).to_be_bytes())?;
                out.write_all(&name[..name.len().min(u16::MAX as usize)])?;
                out.write_all(&w.creation_timestamp.to_be_bytes())?;
                out.write_all(&w.location.x.to_be_bytes())?;
                out.write_all(&w.location.y.to_be_bytes())?;
                out.write_all(&w.location.z.to_be_bytes())?;
            }
            out.flush()
        })();
        if let Err(e) = result {
            log::warn!("[Baritone] couldn't save {} waypoints: {e}", tag.name());
        }
    }

    pub fn add(&mut self, waypoint: Waypoint) {
        if self.waypoints.contains(&waypoint) {
            return;
        }
        let tag = waypoint.tag;
        self.waypoints.push(waypoint);
        self.save(tag);
    }

    pub fn remove(&mut self, waypoint: &Waypoint) -> bool {
        let before = self.waypoints.len();
        self.waypoints.retain(|w| w != waypoint);
        let removed = self.waypoints.len() != before;
        if removed {
            self.save(waypoint.tag);
        }
        removed
    }

    /// The newest waypoint with a tag (`getMostRecentByTag`).
    pub fn most_recent(&self, tag: Tag) -> Option<&Waypoint> {
        self.waypoints
            .iter()
            .filter(|w| w.tag == tag)
            .max_by_key(|w| w.creation_timestamp)
    }

    pub fn by_tag(&self, tag: Tag) -> impl Iterator<Item = &Waypoint> {
        self.waypoints.iter().filter(move |w| w.tag == tag)
    }

    pub fn all(&self) -> &[Waypoint] {
        &self.waypoints
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waypoints_save_and_load() {
        let dir = std::env::temp_dir().join(format!("baritone-wp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        {
            let mut wps = WaypointCollection::open(dir.clone());
            wps.add(Waypoint::new("base", Tag::Home, BlockPos::new(1, 64, -3)));
            wps.add(Waypoint::new("spot", Tag::User, BlockPos::new(-100, 12, 9)));
            wps.add(Waypoint::new("spot", Tag::User, BlockPos::new(-100, 12, 9)));
            assert_eq!(wps.all().len(), 2);
        }
        let wps = WaypointCollection::open(dir.clone());
        assert_eq!(wps.all().len(), 2);
        assert_eq!(wps.most_recent(Tag::Home).unwrap().location, BlockPos::new(1, 64, -3));
        assert_eq!(Tag::by_name("BASE"), Some(Tag::Home));
        assert_eq!(Tag::by_name("spawn"), Some(Tag::Bed));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
