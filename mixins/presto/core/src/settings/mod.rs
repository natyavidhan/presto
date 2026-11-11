//! Presto's settings: every tunable value Presto reads, each with the name, default and description of
//! Baritone's `Settings.java` (the table in `list.rs` is generated from it), and the `settings.txt` format of
//! `SettingsUtil`: one `name value` line per modified setting.

use std::fmt::Write as _;
use std::sync::atomic::{AtomicBool, Ordering};

static CENSOR_COORDINATES: AtomicBool = AtomicBool::new(false);

/// `censorCoordinates`, readable from anywhere that prints a position.
pub fn censor_coordinates() -> bool {
    CENSOR_COORDINATES.load(Ordering::Relaxed)
}

pub fn set_censor_coordinates(on: bool) {
    CENSOR_COORDINATES.store(on, Ordering::Relaxed);
}

/// `SettingsUtil.maybeCensor`.
pub fn maybe_censor(coord: i32) -> String {
    if censor_coordinates() {
        "<censored>".into()
    } else {
        coord.to_string()
    }
}

/// A colour setting, written `r,g,b` as `SettingsUtil` writes `java.awt.Color`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// `0xAARRGGBB` with the given alpha.
    pub const fn argb(self, alpha: u8) -> u32 {
        (alpha as u32) << 24 | (self.r as u32) << 16 | (self.g as u32) << 8 | self.b as u32
    }

    /// `0xAARRGGBB` with alpha from a 0..=1 opacity.
    pub fn argb_f(self, alpha: f32) -> u32 {
        self.argb((alpha.clamp(0.0, 1.0) * 255.0).round() as u8)
    }
}

/// Normalises a block or item id the way `Identifier.parse` reads one: a bare path is `minecraft:`'s.
pub fn normalize_id(id: &str) -> String {
    let id = id.trim().to_ascii_lowercase();
    if id.contains(':') {
        id
    } else {
        format!("minecraft:{id}")
    }
}

/// A list of block ids (`List<Block>` in Java).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BlockList(pub Vec<String>);

impl BlockList {
    pub fn of(ids: &[&str]) -> Self {
        Self(ids.iter().map(|s| normalize_id(s)).collect())
    }
    pub fn contains(&self, name: &str) -> bool {
        self.0.iter().any(|b| b == name)
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(String::as_str)
    }
}

/// A list of item ids (`List<Item>` in Java).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemList(pub Vec<String>);

impl ItemList {
    pub fn of(ids: &[&str]) -> Self {
        Self(ids.iter().map(|s| normalize_id(s)).collect())
    }
    pub fn contains(&self, name: &str) -> bool {
        self.0.iter().any(|b| b == name)
    }
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(String::as_str)
    }
}

/// A list of plain strings (`List<String>`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StringList(pub Vec<String>);

impl StringList {
    pub fn contains(&self, s: &str) -> bool {
        self.0.iter().any(|b| b == s)
    }
}

/// A block to blocks mapping (`Map<Block, List<Block>>`), in insertion order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BlockMap(pub Vec<(String, Vec<String>)>);

impl BlockMap {
    pub fn get(&self, key: &str) -> Option<&[String]> {
        self.0
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_slice())
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// `net.minecraft.world.level.block.Rotation`, for `buildSchematicRotation`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SchematicRotation {
    #[default]
    None,
    Clockwise90,
    Clockwise180,
    Counterclockwise90,
}

impl SchematicRotation {
    pub const ALL: [SchematicRotation; 4] = [
        SchematicRotation::None,
        SchematicRotation::Clockwise90,
        SchematicRotation::Clockwise180,
        SchematicRotation::Counterclockwise90,
    ];
    pub fn name(self) -> &'static str {
        match self {
            SchematicRotation::None => "NONE",
            SchematicRotation::Clockwise90 => "CLOCKWISE_90",
            SchematicRotation::Clockwise180 => "CLOCKWISE_180",
            SchematicRotation::Counterclockwise90 => "COUNTERCLOCKWISE_90",
        }
    }
    /// Quarter turns clockwise.
    pub fn quarter_turns(self) -> u8 {
        match self {
            SchematicRotation::None => 0,
            SchematicRotation::Clockwise90 => 1,
            SchematicRotation::Clockwise180 => 2,
            SchematicRotation::Counterclockwise90 => 3,
        }
    }
}

/// `net.minecraft.world.level.block.Mirror`, for `buildSchematicMirror`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SchematicMirror {
    #[default]
    None,
    LeftRight,
    FrontBack,
}

impl SchematicMirror {
    pub const ALL: [SchematicMirror; 3] = [
        SchematicMirror::None,
        SchematicMirror::LeftRight,
        SchematicMirror::FrontBack,
    ];
    pub fn name(self) -> &'static str {
        match self {
            SchematicMirror::None => "NONE",
            SchematicMirror::LeftRight => "LEFT_RIGHT",
            SchematicMirror::FrontBack => "FRONT_BACK",
        }
    }
}

/// How a setting's value is read from and written to text, as `SettingsUtil.Parser` does.
pub trait SettingValue: Clone + PartialEq {
    /// The type as `#set` describes it (`SettingsUtil.settingTypeToString`).
    const TYPE_NAME: &'static str;
    fn parse(raw: &str) -> Result<Self, String>;
    fn write(&self, out: &mut String);
    fn display(&self) -> String {
        let mut s = String::new();
        self.write(&mut s);
        s
    }
    fn as_bool(&self) -> Option<bool> {
        None
    }
}

impl SettingValue for bool {
    const TYPE_NAME: &'static str = "Boolean";
    fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "true" => Ok(true),
            "false" => Ok(false),
            other => Err(format!("'{other}' is not true or false")),
        }
    }
    fn write(&self, out: &mut String) {
        out.push_str(if *self { "true" } else { "false" });
    }
    fn as_bool(&self) -> Option<bool> {
        Some(*self)
    }
}

macro_rules! number_setting {
    ($ty:ty, $name:literal) => {
        impl SettingValue for $ty {
            const TYPE_NAME: &'static str = $name;
            fn parse(raw: &str) -> Result<Self, String> {
                let raw = raw.trim();
                raw.parse::<$ty>()
                    .map_err(|e| format!("'{raw}' is not a valid {}: {e}", $name))
            }
            fn write(&self, out: &mut String) {
                let _ = write!(out, "{self}");
            }
        }
    };
}
number_setting!(i32, "Integer");
number_setting!(i64, "Long");

macro_rules! float_setting {
    ($ty:ty, $name:literal) => {
        impl SettingValue for $ty {
            const TYPE_NAME: &'static str = $name;
            fn parse(raw: &str) -> Result<Self, String> {
                let raw = raw.trim();
                // Java's parseDouble also takes a trailing d/f type suffix.
                let digits = raw.trim_end_matches(['d', 'D', 'f', 'F']);
                digits
                    .parse::<$ty>()
                    .map_err(|e| format!("'{raw}' is not a valid {}: {e}", $name))
            }
            fn write(&self, out: &mut String) {
                // Java prints whole doubles with a trailing ".0".
                if self.fract() == 0.0 && self.is_finite() && self.abs() < 1e15 {
                    let _ = write!(out, "{self:.1}");
                } else {
                    let _ = write!(out, "{self}");
                }
            }
        }
    };
}
float_setting!(f32, "Float");
float_setting!(f64, "Double");

impl SettingValue for String {
    const TYPE_NAME: &'static str = "String";
    fn parse(raw: &str) -> Result<Self, String> {
        Ok(raw.to_string())
    }
    fn write(&self, out: &mut String) {
        out.push_str(self);
    }
}

impl SettingValue for Color {
    const TYPE_NAME: &'static str = "Color";
    fn parse(raw: &str) -> Result<Self, String> {
        let parts: Vec<&str> = raw.split(',').collect();
        if parts.len() < 3 {
            return Err(format!("'{raw}' is not a colour: write it r,g,b"));
        }
        let c = |s: &str| -> Result<u8, String> {
            let v: i32 = s
                .trim()
                .parse()
                .map_err(|e| format!("'{s}' is not a number: {e}"))?;
            u8::try_from(v).map_err(|_| format!("colour component {v} is outside 0..=255"))
        };
        Ok(Color::rgb(c(parts[0])?, c(parts[1])?, c(parts[2])?))
    }
    fn write(&self, out: &mut String) {
        let _ = write!(out, "{},{},{}", self.r, self.g, self.b);
    }
}

impl SettingValue for [i32; 3] {
    const TYPE_NAME: &'static str = "Vec3i";
    fn parse(raw: &str) -> Result<Self, String> {
        let parts: Vec<&str> = raw.split(',').collect();
        if parts.len() < 3 {
            return Err(format!("'{raw}' is not a vector: write it x,y,z"));
        }
        let n = |s: &str| -> Result<i32, String> {
            s.trim()
                .parse()
                .map_err(|e| format!("'{s}' is not a number: {e}"))
        };
        Ok([n(parts[0])?, n(parts[1])?, n(parts[2])?])
    }
    fn write(&self, out: &mut String) {
        let _ = write!(out, "{},{},{}", self[0], self[1], self[2]);
    }
}

fn parse_id_list(raw: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for part in raw.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if !part
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | ':' | '/' | '.' | '-'))
        {
            return Err(format!("'{part}' is not a valid id"));
        }
        out.push(normalize_id(part));
    }
    Ok(out)
}

impl SettingValue for BlockList {
    const TYPE_NAME: &'static str = "List<Block>";
    fn parse(raw: &str) -> Result<Self, String> {
        parse_id_list(raw).map(BlockList)
    }
    fn write(&self, out: &mut String) {
        out.push_str(&self.0.join(","));
    }
}

impl SettingValue for ItemList {
    const TYPE_NAME: &'static str = "List<Item>";
    fn parse(raw: &str) -> Result<Self, String> {
        parse_id_list(raw).map(ItemList)
    }
    fn write(&self, out: &mut String) {
        out.push_str(&self.0.join(","));
    }
}

impl SettingValue for StringList {
    const TYPE_NAME: &'static str = "List<String>";
    fn parse(raw: &str) -> Result<Self, String> {
        Ok(StringList(
            raw.split(',')
                .map(|s| s.to_string())
                .filter(|s| !s.is_empty())
                .collect(),
        ))
    }
    fn write(&self, out: &mut String) {
        out.push_str(&self.0.join(","));
    }
}

impl SettingValue for BlockMap {
    const TYPE_NAME: &'static str = "Map<Block, List<Block>>";
    fn parse(raw: &str) -> Result<Self, String> {
        // `SettingsUtil.Parser.MAPPING`: entries split on the commas that come before a `->`.
        let mut entries = Vec::new();
        let mut rest = raw;
        while !rest.trim().is_empty() {
            let arrow = rest
                .find("->")
                .ok_or_else(|| format!("'{rest}' is missing a '->'"))?;
            let key = &rest[..arrow];
            let after = &rest[arrow + 2..];
            // The value runs to the last comma before the next "->", or to the end.
            let end = match after.find("->") {
                Some(next_arrow) => after[..next_arrow].rfind(',').unwrap_or(after.len()),
                None => after.len(),
            };
            let value = &after[..end];
            let key = parse_id_list(key)?
                .into_iter()
                .next()
                .ok_or_else(|| "empty key in mapping".to_string())?;
            entries.push((key, parse_id_list(value)?));
            rest = if end < after.len() {
                &after[end + 1..]
            } else {
                ""
            };
        }
        Ok(BlockMap(entries))
    }
    fn write(&self, out: &mut String) {
        let mut first = true;
        for (k, v) in &self.0 {
            if !first {
                out.push(',');
            }
            first = false;
            let _ = write!(out, "{k}->{}", v.join(","));
        }
    }
}

impl SettingValue for SchematicRotation {
    const TYPE_NAME: &'static str = "Rotation";
    fn parse(raw: &str) -> Result<Self, String> {
        let raw = raw.trim().to_ascii_uppercase();
        SchematicRotation::ALL
            .into_iter()
            .find(|r| r.name() == raw)
            .ok_or_else(|| {
                format!("'{raw}' is not NONE, CLOCKWISE_90, CLOCKWISE_180 or COUNTERCLOCKWISE_90")
            })
    }
    fn write(&self, out: &mut String) {
        out.push_str(self.name());
    }
}

impl SettingValue for SchematicMirror {
    const TYPE_NAME: &'static str = "Mirror";
    fn parse(raw: &str) -> Result<Self, String> {
        let raw = raw.trim().to_ascii_uppercase();
        SchematicMirror::ALL
            .into_iter()
            .find(|r| r.name() == raw)
            .ok_or_else(|| format!("'{raw}' is not NONE, LEFT_RIGHT or FRONT_BACK"))
    }
    fn write(&self, out: &mut String) {
        out.push_str(self.name());
    }
}

/// One setting seen without its Rust type: what `#set`, `#modified` and `settings.txt` work with.
pub struct SettingMeta {
    /// The name as Baritone spells it (`allowBreak`); commands match it ignoring case.
    pub name: &'static str,
    /// Its description: Baritone's, with the bot called Presto.
    pub description: &'static str,
    pub type_name: &'static str,
    pub value: fn(&Settings) -> String,
    pub default_value: fn() -> String,
    pub is_default: fn(&Settings) -> bool,
    pub set: fn(&mut Settings, &str) -> Result<(), String>,
    pub reset: fn(&mut Settings),
    /// The value of a Boolean setting.
    pub as_bool: fn(&Settings) -> Option<bool>,
}

impl SettingMeta {
    /// `true` for a Boolean setting.
    pub fn is_bool(&self) -> bool {
        self.type_name == "Boolean"
    }
    /// `SettingsUtil.settingToString`.
    pub fn line(&self, settings: &Settings) -> String {
        format!("{} {}", self.name, (self.value)(settings))
    }
}

macro_rules! define_settings {
    ($( $(#[doc = $doc:expr])* $field:ident : $ty:ty = $default:expr => $name:literal; )*) => {
        /// Every setting, as plain fields: read them directly (`settings.allow_break`).
        #[derive(Clone, Debug, PartialEq)]
        pub struct Settings {
            $( $(#[doc = $doc])* pub $field: $ty, )*
        }

        impl Default for Settings {
            fn default() -> Self {
                Self { $( $field: $default, )* }
            }
        }

        /// The settings in Baritone's order.
        pub static ALL_SETTINGS: &[SettingMeta] = &[
            $(
                SettingMeta {
                    name: $name,
                    description: concat!($($doc, "\n",)* ""),
                    type_name: <$ty as SettingValue>::TYPE_NAME,
                    value: |s| SettingValue::display(&s.$field),
                    default_value: || { let d: $ty = $default; SettingValue::display(&d) },
                    is_default: |s| { let d: $ty = $default; s.$field == d },
                    set: |s, raw| { s.$field = <$ty as SettingValue>::parse(raw)?; Ok(()) },
                    reset: |s| { s.$field = $default; },
                    as_bool: |s| SettingValue::as_bool(&s.$field),
                },
            )*
        ];
    };
}

include!("list.rs");

impl Settings {
    /// The setting named `name`, ignoring case (`Settings.byLowerName`).
    pub fn meta(name: &str) -> Option<&'static SettingMeta> {
        let name = name.trim();
        // Old names still read: allowJumpAt256 (as `SettingsUtil.readAndApply` does), and Baritone's name for
        // the short chat prefix, so a Baritone settings file works here.
        let name = if name.eq_ignore_ascii_case("allowjumpat256") {
            "allowJumpAtBuildLimit"
        } else if name.eq_ignore_ascii_case("shortbaritoneprefix") {
            "shortPrestoPrefix"
        } else {
            name
        };
        ALL_SETTINGS
            .iter()
            .find(|m| m.name.eq_ignore_ascii_case(name))
    }

    pub fn all() -> &'static [SettingMeta] {
        ALL_SETTINGS
    }

    /// The settings that differ from their defaults (`SettingsUtil.modifiedSettings`).
    pub fn modified(&self) -> Vec<&'static SettingMeta> {
        ALL_SETTINGS
            .iter()
            .filter(|m| !(m.is_default)(self))
            .collect()
    }

    /// Parses and applies `name value`, as `SettingsUtil.parseAndApply` does.
    pub fn parse_and_apply(&mut self, name: &str, value: &str) -> Result<(), String> {
        let meta = Self::meta(name).ok_or_else(|| format!("No setting by the name {name}"))?;
        (meta.set)(self, value)
    }

    /// The `settings.txt` text: one line per modified setting (`SettingsUtil.save`).
    pub fn to_file(&self) -> String {
        let mut out = String::new();
        for meta in self.modified() {
            out.push_str(&meta.line(self));
            out.push('\n');
        }
        out
    }

    /// Applies a `settings.txt` (`SettingsUtil.readAndApply`): `#` and `//` lines are comments, a key and
    /// its value are split at the first space. Returns the messages for lines it couldn't apply.
    pub fn apply_file(&mut self, text: &str) -> Vec<String> {
        let mut errors = Vec::new();
        for line in text.lines() {
            if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
                continue;
            }
            let Some((name, value)) = line.split_once(' ') else {
                errors.push(format!("Invalid syntax in setting file: {line}"));
                continue;
            };
            let value = value.trim_start_matches(' ');
            if value.is_empty() {
                errors.push(format!("Invalid syntax in setting file: {line}"));
                continue;
            }
            if let Err(e) = self.parse_and_apply(name, value) {
                errors.push(format!("Unable to parse line {line}: {e}"));
            }
        }
        errors
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_baritone() {
        let s = Settings::default();
        assert!(s.allow_break);
        assert!(!s.allow_inventory);
        assert_eq!(s.block_placement_penalty, 20.0);
        assert_eq!(s.prefix, "#");
        assert_eq!(s.color_current_path, Color::rgb(255, 0, 0));
        assert!(s.blocks_to_avoid_breaking.contains("minecraft:crafting_table"));
        assert!(s.acceptable_throwaway_items.contains("minecraft:netherrack"));
        assert_eq!(s.build_repeat, [0, 0, 0]);
        assert!(ALL_SETTINGS.len() > 240);
        assert!(s.modified().is_empty());
    }

    #[test]
    fn names_are_unique_ignoring_case() {
        let mut names: Vec<String> = ALL_SETTINGS
            .iter()
            .map(|m| m.name.to_ascii_lowercase())
            .collect();
        names.sort();
        let len = names.len();
        names.dedup();
        assert_eq!(len, names.len());
    }

    #[test]
    fn round_trips_through_the_settings_file() {
        let mut s = Settings::default();
        s.parse_and_apply("ALLOWBREAK", "false").unwrap();
        s.parse_and_apply("colorGoalBox", "1,2,3").unwrap();
        s.parse_and_apply("blocksToAvoid", "stone,minecraft:dirt").unwrap();
        s.parse_and_apply("buildSubstitutes", "stone->dirt,cobblestone,glass->air")
            .unwrap();
        s.parse_and_apply("blockReachDistance", "4.0").unwrap();
        s.parse_and_apply("buildSchematicRotation", "clockwise_90")
            .unwrap();
        let text = s.to_file();
        let mut t = Settings::default();
        assert!(t.apply_file(&text).is_empty(), "{text}");
        assert_eq!(s, t);
        assert!(!t.allow_break);
        assert_eq!(
            t.build_substitutes.get("minecraft:stone").unwrap(),
            &["minecraft:dirt".to_string(), "minecraft:cobblestone".to_string()]
        );
        assert_eq!(
            t.build_substitutes.get("minecraft:glass").unwrap(),
            &["minecraft:air".to_string()]
        );
        assert!(text.contains("blockReachDistance 4.0"));
    }

    #[test]
    fn rejects_bad_values() {
        let mut s = Settings::default();
        assert!(s.parse_and_apply("allowBreak", "maybe").is_err());
        assert!(s.parse_and_apply("maxFallHeightNoWater", "x").is_err());
        assert!(s.parse_and_apply("noSuchSetting", "1").is_err());
        assert!(s.modified().is_empty());
    }
}
