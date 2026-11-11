//! `BlockOptionalMeta` and `BlockOptionalMetaLookup`: block selectors as the commands take them (`stone`,
//! `minecraft:oak_log[axis=y]`), compiled to a bitset over state ids so matching a state is one bit test.

use crate::world::{BlockId, BlockTable, StateId};
use std::fmt;
use std::sync::Arc;

/// A set of block states, one bit each.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct StateSet {
    bits: Vec<u64>,
    count: usize,
}

impl StateSet {
    pub fn with_capacity(states: usize) -> Self {
        Self {
            bits: vec![0; states.div_ceil(64)],
            count: 0,
        }
    }
    #[inline]
    pub fn contains(&self, s: StateId) -> bool {
        let i = s.0 as usize;
        self.bits
            .get(i >> 6)
            .is_some_and(|w| w & (1 << (i & 63)) != 0)
    }
    pub fn insert(&mut self, s: StateId) {
        let i = s.0 as usize;
        if i >> 6 >= self.bits.len() {
            self.bits.resize((i >> 6) + 1, 0);
        }
        let w = &mut self.bits[i >> 6];
        if *w & (1 << (i & 63)) == 0 {
            *w |= 1 << (i & 63);
            self.count += 1;
        }
    }
    pub fn union(&mut self, o: &StateSet) {
        if o.bits.len() > self.bits.len() {
            self.bits.resize(o.bits.len(), 0);
        }
        for (a, b) in self.bits.iter_mut().zip(&o.bits) {
            *a |= *b;
        }
        self.count = self.bits.iter().map(|w| w.count_ones() as usize).sum();
    }
    pub fn len(&self) -> usize {
        self.count
    }
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
    pub fn iter(&self) -> impl Iterator<Item = StateId> + '_ {
        self.bits.iter().enumerate().flat_map(|(wi, w)| {
            let mut w = *w;
            std::iter::from_fn(move || {
                if w == 0 {
                    return None;
                }
                let b = w.trailing_zeros();
                w &= w - 1;
                Some(StateId((wi * 64) as u32 + b))
            })
        })
    }
}

impl fmt::Debug for StateSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "StateSet({} states)", self.count)
    }
}

/// `BlockOptionalMeta`: one block, optionally narrowed to states with some property values.
#[derive(Clone)]
pub struct BlockOptionalMeta {
    pub block: BlockId,
    pub name: Arc<str>,
    /// `{axis:y}`, for display.
    properties_description: String,
    states: StateSet,
    /// The items breaking it drops (`getStackHashes`), to recognise its drops on the ground.
    drops: Vec<Arc<str>>,
}

impl BlockOptionalMeta {
    /// Every state of a block.
    pub fn of_block(table: &BlockTable, block: BlockId) -> Self {
        let def = table.block(block);
        let mut states = StateSet::with_capacity(table.len());
        for s in &def.states {
            states.insert(*s);
        }
        Self {
            block,
            name: def.name.clone(),
            properties_description: "{}".into(),
            states,
            drops: table.drops(block).to_vec(),
        }
    }

    /// Parses `id`, `id[]` or `id[prop=value,...]`.
    pub fn parse(table: &BlockTable, selector: &str) -> Result<Self, String> {
        let selector = selector.trim();
        let (id, props) = match selector.find('[') {
            Some(i) if selector.ends_with(']') => (&selector[..i], Some(&selector[i + 1..selector.len() - 1])),
            _ => (selector, None),
        };
        if id.is_empty() {
            return Err("invalid block selector".into());
        }
        let block = table
            .block_id(&id.to_ascii_lowercase())
            .ok_or_else(|| format!("Invalid block ID {id}"))?;
        let mut wanted: Vec<(String, String)> = Vec::new();
        if let Some(props) = props.filter(|p| !p.is_empty()) {
            for pair in props.split(',') {
                let parts: Vec<&str> = pair.split('=').collect();
                if parts.len() != 2 {
                    return Err(format!("\"{pair}\" is not a valid property-value pair"));
                }
                let (k, v) = (parts[0].trim(), parts[1].trim());
                let def = table.block(block);
                let base = table.get(def.default_state);
                if base.property(k).is_none() {
                    return Err(format!("\"{k}\" is not a property of {}", def.name));
                }
                if !def.states.iter().any(|s| table.get(*s).property(k) == Some(v)) {
                    return Err(format!("\"{v}\" is not a valid value for {k} on {}", def.name));
                }
                wanted.push((k.to_string(), v.to_string()));
            }
        }
        let def = table.block(block);
        let mut states = StateSet::with_capacity(table.len());
        for s in &def.states {
            let info = table.get(*s);
            if wanted.iter().all(|(k, v)| info.property(k) == Some(v.as_str())) {
                states.insert(*s);
            }
        }
        Ok(Self {
            block,
            name: def.name.clone(),
            properties_description: match props {
                None => "{}".into(),
                Some(p) => format!("{{{}}}", p.replace('=', ":")),
            },
            states,
            drops: table.drops(block).to_vec(),
        })
    }

    #[inline]
    pub fn matches(&self, state: StateId) -> bool {
        self.states.contains(state)
    }

    pub fn matches_block(&self, block: BlockId) -> bool {
        self.block == block
    }

    /// Whether an item is one this block drops (`matches(ItemStack)`).
    pub fn matches_item(&self, item: &str) -> bool {
        self.drops.iter().any(|d| &**d == item)
    }

    pub fn states(&self) -> &StateSet {
        &self.states
    }

    pub fn any_state(&self) -> Option<StateId> {
        self.states.iter().next()
    }
}

impl fmt::Debug for BlockOptionalMeta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl fmt::Display for BlockOptionalMeta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "BlockOptionalMeta{{block={},properties={}}}",
            self.name, self.properties_description
        )
    }
}

/// `BlockOptionalMetaLookup`: several selectors at once.
#[derive(Clone, Default)]
pub struct BlockOptionalMetaLookup {
    boms: Vec<BlockOptionalMeta>,
    states: StateSet,
    blocks: Vec<BlockId>,
}

impl BlockOptionalMetaLookup {
    pub fn new(boms: Vec<BlockOptionalMeta>) -> Self {
        let mut states = StateSet::default();
        let mut blocks = Vec::new();
        for b in &boms {
            states.union(&b.states);
            if !blocks.contains(&b.block) {
                blocks.push(b.block);
            }
        }
        Self { boms, states, blocks }
    }

    pub fn parse(table: &BlockTable, selectors: &[&str]) -> Result<Self, String> {
        let boms = selectors
            .iter()
            .map(|s| BlockOptionalMeta::parse(table, s))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self::new(boms))
    }

    #[inline]
    pub fn has(&self, state: StateId) -> bool {
        self.states.contains(state)
    }

    pub fn has_block(&self, block: BlockId) -> bool {
        self.blocks.contains(&block)
    }

    pub fn has_item(&self, item: &str) -> bool {
        self.boms.iter().any(|b| b.matches_item(item))
    }

    pub fn blocks(&self) -> &[BlockOptionalMeta] {
        &self.boms
    }

    pub fn states(&self) -> &StateSet {
        &self.states
    }

    pub fn is_empty(&self) -> bool {
        self.boms.is_empty()
    }
}

impl fmt::Debug for BlockOptionalMetaLookup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl fmt::Display for BlockOptionalMetaLookup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BlockOptionalMetaLookup{{[")?;
        for (i, b) in self.boms.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{b}")?;
        }
        write!(f, "]}}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::block::testing;

    #[test]
    fn parses_selectors() {
        let t = testing::vanilla();
        let all = BlockOptionalMeta::parse(&t, "oak_stairs").unwrap();
        assert_eq!(all.states().len(), 2);
        let top = BlockOptionalMeta::parse(&t, "minecraft:oak_stairs[half=top]").unwrap();
        assert_eq!(top.states().len(), 1);
        assert!(BlockOptionalMeta::parse(&t, "oak_stairs[nope=1]").is_err());
        assert!(BlockOptionalMeta::parse(&t, "oak_stairs[half=sideways]").is_err());
        assert!(BlockOptionalMeta::parse(&t, "not_a_block").is_err());
        let lookup = BlockOptionalMetaLookup::parse(&t, &["stone", "dirt"]).unwrap();
        assert!(lookup.has(t.default_state("minecraft:stone").unwrap()));
        assert!(!lookup.has(t.default_state("minecraft:sand").unwrap()));
    }
}
