//! Generate Minecraft litematic projections from NBS songs.

use crate::note::{ImitateInstrument, Instrument, Tone};
use itertools::iproduct;
use mcdata::BlockState;
use mcdata::{GenericBlockState, util::BlockPos};
use rustmatica::{Litematic, Region};
use std::borrow::Cow;
use std::collections::HashMap;

mod compact;
mod linear;
mod tapped;
pub use compact::*;
pub use linear::*;
pub use tapped::*;

// SchematicBuilder
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Output a [`Layout`] as a litematic file.
///
/// Example: `SchematicBuilder(layout).build("Song", "Me")`
pub struct SchematicBuilder<L: Layout>(pub L);

impl<L: Layout> SchematicBuilder<L> {
    /// Iterate every position in the layout's bounding box and produce a litematic.
    pub fn build(
        self,
        description: impl Into<Cow<'static, str>>,
        author: impl Into<Cow<'static, str>>,
    ) -> Litematic {
        let SchematicBuilder(layout) = self;
        let size = layout.size();
        const NAME: &str = "Note Block Track Schematic";
        let mut region: Region<GenericBlockState> = Region::new(NAME, BlockPos::ORIGIN, size);

        for (y, z, x) in iproduct!(0..size.y, 0..size.z, 0..size.x) {
            let pos = BlockPos::new(x, y, z);
            region.set_block(pos, layout.get_block(pos));
        }
        region.as_litematic(description, author)
    }
}

// Layout trait
//
// ++++++++++++============++++++++++++============++++++++++++============

/// A queryable projection layout.
pub trait Layout {
    /// Total size of the bounding box.
    fn size(&self) -> BlockPos;
    /// Block at the given world position.
    fn get_block(&self, pos: BlockPos) -> GenericBlockState;
}

// EdgeArranged
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Like [`Arranged`], but aligns sub-layouts by their far-edge on cross-axes.
pub struct EdgeArranged<L: Layout> {
    inner: Reverse<Arranged<Reverse<L>>>,
}

impl<L: Layout> EdgeArranged<L> {
    /// `align` is passed to both inner (per-sub-layout) and outer (whole) Reverse.
    pub fn new<I: IntoIterator<Item = L>>(layouts: I, axis: Axis, gap: u32, align: Mask) -> Self {
        let reversed = layouts.into_iter().map(|l| Reverse::new(l, align));
        let arranged = Arranged::new(reversed, axis, gap);
        let inner = Reverse::new(arranged, align);
        Self { inner }
    }
}

impl<L: Layout> Layout for EdgeArranged<L> {
    fn size(&self) -> BlockPos {
        self.inner.size()
    }

    fn get_block(&self, pos: BlockPos) -> GenericBlockState {
        self.inner.get_block(pos)
    }
}

// Arranged
//
// ++++++++++++============++++++++++++============++++++++++++============

/// A layout wrapper that arranges sub-layouts along an [`Axis`].
pub struct Arranged<L: Layout> {
    bands: Vec<(L, BlockPos)>,
    size: BlockPos,
}

impl<L: Layout> Arranged<L> {
    pub fn new<I: IntoIterator<Item = L>>(layouts: I, axis: Axis, gap: u32) -> Self {
        let unit: Mask = axis.unit();
        let gap_vec: BlockPos = unit * gap as i32;
        let mut cursor: BlockPos = -gap_vec;
        let mut extent: BlockPos = BlockPos::ORIGIN;

        let placed = layouts.into_iter().map(|layout| {
            let size: BlockPos = layout.size();
            let anchor: BlockPos = cursor + gap_vec;
            cursor = anchor + unit * size;
            extent = Self::_max(extent, size);
            (layout, anchor)
        });

        let bands = placed.collect();
        let size = Self::_max(Self::_max(cursor, BlockPos::ORIGIN), extent);
        Self { bands, size }
    }

    fn _max(a: BlockPos, b: BlockPos) -> BlockPos {
        BlockPos::new(a.x.max(b.x), a.y.max(b.y), a.z.max(b.z))
    }
}

impl<L: Layout> Layout for Arranged<L> {
    fn size(&self) -> BlockPos {
        self.size
    }

    fn get_block(&self, pos: BlockPos) -> GenericBlockState {
        debug_assert!((0..self.size.x).contains(&pos.x), "x out of range");
        debug_assert!((0..self.size.y).contains(&pos.y), "y out of range");
        debug_assert!((0..self.size.z).contains(&pos.z), "z out of range");

        let found = self
            .bands
            .partition_point(|(_, a)| a.y <= pos.y && a.z <= pos.z && a.x <= pos.x)
            .checked_sub(1);
        let Some(index) = found else { return air() };

        let (layout, anchor) = &self.bands[index];
        let local = BlockPos::new(pos.x - anchor.x, pos.y - anchor.y, pos.z - anchor.z);
        let size = layout.size();
        let hit = (0..size.x).contains(&local.x)
            && (0..size.y).contains(&local.y)
            && (0..size.z).contains(&local.z);
        if hit { layout.get_block(local) } else { air() }
    }
}

// Anchored
//
// ++++++++++++============++++++++++++============++++++++++++============

/// A layout wrapper that places sub-layouts at explicit anchor positions.
#[deprecated(note = "is too slow for hot paths; use `Arranged` instead")]
pub struct Anchored<L: Layout> {
    entries: Vec<(L, BlockPos)>,
    size: BlockPos,
}

#[allow(deprecated)]
impl<L: Layout> Anchored<L> {
    pub fn new<I: IntoIterator<Item = (L, BlockPos)>>(entries: I) -> Self {
        let mut extent = BlockPos::ORIGIN;

        let placed = entries.into_iter().map(|(layout, anchor)| {
            let size = layout.size();
            let far = anchor + size;
            extent = _component_max(extent, far);
            (layout, anchor)
        });

        let entries = placed.collect();
        let size = extent;
        Self { entries, size }
    }
}

#[allow(deprecated)]
impl<L: Layout> Layout for Anchored<L> {
    fn size(&self) -> BlockPos {
        self.size
    }

    fn get_block(&self, pos: BlockPos) -> GenericBlockState {
        debug_assert!((0..self.size.x).contains(&pos.x), "x out of range");
        debug_assert!((0..self.size.y).contains(&pos.y), "y out of range");
        debug_assert!((0..self.size.z).contains(&pos.z), "z out of range");

        let found = self.entries.iter().find_map(|(layout, anchor)| {
            let local = pos - *anchor;
            let size = layout.size();
            let hit = (0..size.x).contains(&local.x)
                && (0..size.y).contains(&local.y)
                && (0..size.z).contains(&local.z);
            hit.then(|| layout.get_block(local))
        });

        found.unwrap_or_else(air)
    }
}

// Reverse
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Mirror-reverse a layout along given axes. Block facing unchanged.
pub struct Reverse<L: Layout> {
    layout: L,
    sign: Mask,
}

impl<L: Layout> Reverse<L> {
    pub fn new(layout: L, sign: Mask) -> Self {
        Self { layout, sign }
    }
}

impl<L: Layout> Layout for Reverse<L> {
    fn size(&self) -> BlockPos {
        self.layout.size()
    }

    fn get_block(&self, pos: BlockPos) -> GenericBlockState {
        let size = self.layout.size();
        let orig = pos + self.sign * (size - BlockPos::new(1, 1, 1) - pos * 2);
        debug_assert!((0..size.x).contains(&orig.x));
        debug_assert!((0..size.y).contains(&orig.y));
        debug_assert!((0..size.z).contains(&orig.z));
        self.layout.get_block(orig)
    }
}

// WithFloor
//
// ++++++++++++============++++++++++++============++++++++++++============

/// A layout wrapper that adds a floor layer beneath another layout.
pub struct WithFloor<L: Layout> {
    layout: L,
    full: bool,
}

impl<L: Layout> WithFloor<L> {
    /// Whether the floor fully covers the entire bounding box.
    /// When `false`, only positions with a block above get a floor.
    pub fn new(layout: L, full: bool) -> Self {
        Self { layout, full }
    }
}

impl<L: Layout> Layout for WithFloor<L> {
    fn size(&self) -> BlockPos {
        let size = self.layout.size();
        BlockPos::new(size.x, size.y + 1, size.z)
    }

    fn get_block(&self, pos: BlockPos) -> GenericBlockState {
        debug_assert!((0..self.size().x).contains(&pos.x), "x out of range");
        debug_assert!((0..self.size().y).contains(&pos.y), "y out of range");
        debug_assert!((0..self.size().z).contains(&pos.z), "z out of range");

        let floor = || match self.full {
            true => floor_block(),
            false if self.layout.get_block(pos).name == "minecraft:air" => air(),
            false => floor_block(),
        };
        let local_pos = || BlockPos::new(pos.x, pos.y - 1, pos.z);

        match pos.y {
            0 => floor(),
            _ => self.layout.get_block(local_pos()),
        }
    }
}

// Axis
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Which spatial direction sub-layouts are placed along.
#[derive(Clone, Copy)]
pub enum Axis {
    /// East-west axis (X).
    Easting,
    /// Vertical axis (Y).
    Elevation,
    /// South-north axis (Z).
    Southing,
}

impl Axis {
    /// Unit mask vector for this axis.
    pub fn unit(self) -> Mask {
        match self {
            Axis::Easting => Mask::new(BlockPos::new(1, 0, 0)).unwrap(),
            Axis::Elevation => Mask::new(BlockPos::new(0, 1, 0)).unwrap(),
            Axis::Southing => Mask::new(BlockPos::new(0, 0, 1)).unwrap(),
        }
    }
}

// Mask
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Component-wise mask (0 or 1 on each axis) for BlockPos operations.
///
/// Unlike `BlockPos * BlockPos` (cross product), `Mask * BlockPos` is component-wise.
#[derive(Clone, Copy)]
pub struct Mask(BlockPos);

impl Mask {
    pub fn new(sign: BlockPos) -> Option<Self> {
        match (sign.x == 0 || sign.x == 1)
            && (sign.y == 0 || sign.y == 1)
            && (sign.z == 0 || sign.z == 1)
        {
            true => Some(Self(sign)),
            false => None,
        }
    }
}

impl From<Mask> for BlockPos {
    fn from(s: Mask) -> Self {
        s.0
    }
}

impl std::ops::Mul<BlockPos> for Mask {
    type Output = BlockPos;

    fn mul(self, rhs: BlockPos) -> BlockPos {
        BlockPos::new(self.0.x * rhs.x, self.0.y * rhs.y, self.0.z * rhs.z)
    }
}

impl std::ops::Mul<i32> for Mask {
    type Output = BlockPos;

    fn mul(self, rhs: i32) -> BlockPos {
        self.0 * rhs
    }
}

// Block state projection methods
//
// ++++++++++++============++++++++++++============++++++++++++============

impl Tone {
    /// returns the minecraft note block block state for this tone.
    pub fn note_block_state(&self) -> Option<GenericBlockState> {
        let note = self.key().minecraft_note()?;
        let instr = self.instrument().note_property();
        let properties = HashMap::from([
            ("note".into(), note.to_string().into()),
            ("powered".into(), "false".into()),
            ("instrument".into(), instr.into()),
        ]);
        Some(GenericBlockState {
            name: "minecraft:note_block".into(),
            properties,
        })
    }

    /// returns the block under the note block for this tone's instrument sound.
    pub fn instrument_block_state(&self) -> Option<GenericBlockState> {
        if !self.is_valid() || matches!(self.instrument(), Instrument::Imitate(_)) {
            return None;
        }
        let block = self.instrument().block_resource().unwrap();
        Some(GenericBlockState {
            name: block.into(),
            properties: HashMap::new(),
        })
    }

    /// returns the mob head block for this tone, if it is a mob head instrument.
    pub fn head_block_state(&self) -> Option<GenericBlockState> {
        if !self.is_valid() || !matches!(self.instrument(), Instrument::Imitate(_)) {
            return None;
        }
        let block = self.instrument().block_resource().unwrap();
        Some(GenericBlockState {
            name: block.into(),
            properties: HashMap::new(),
        })
    }
}

impl Instrument {
    /// returns the minecraft instrument property string for note block state.
    pub fn note_property(&self) -> &'static str {
        match self {
            Self::Harp => "harp",
            Self::DoubleBass => "bass",
            Self::BassDrum => "basedrum",
            Self::SnareDrum => "snare",
            Self::Click => "hat",
            Self::Guitar => "guitar",
            Self::Flute => "flute",
            Self::Bell => "bell",
            Self::Chime => "chime",
            Self::Xylophone => "xylophone",
            Self::IronXylophone => "iron_xylophone",
            Self::CowBell => "cow_bell",
            Self::Didgeridoo => "didgeridoo",
            Self::Bit => "bit",
            Self::Banjo => "banjo",
            Self::Pling => "pling",
            Self::Trumpet => "trumpet",
            Self::TrumpetExposed => "trumpet_exposed",
            Self::TrumpetWeathered => "trumpet_weathered",
            Self::TrumpetOxidized => "trumpet_oxidized",
            Self::Imitate(instrument) => instrument.note_property(),
            Self::Other(_) => "custom",
        }
    }

    /// returns the block resource name for this instrument.
    pub fn block_resource(&self) -> Option<&'static str> {
        Some(match self {
            Self::Harp => "minecraft:dirt",
            Self::DoubleBass => "minecraft:oak_planks",
            Self::BassDrum => "minecraft:stone",
            Self::SnareDrum => "minecraft:sand",
            Self::Click => "minecraft:glass",
            Self::Guitar => "minecraft:white_wool",
            Self::Flute => "minecraft:clay",
            Self::Bell => "minecraft:gold_block",
            Self::Chime => "minecraft:packed_ice",
            Self::Xylophone => "minecraft:bone_block",
            Self::IronXylophone => "minecraft:iron_block",
            Self::CowBell => "minecraft:soul_sand",
            Self::Didgeridoo => "minecraft:pumpkin",
            Self::Bit => "minecraft:emerald_block",
            Self::Banjo => "minecraft:hay_block",
            Self::Pling => "minecraft:glowstone",
            Self::Trumpet => "minecraft:waxed_copper_block",
            Self::TrumpetExposed => "minecraft:waxed_exposed_copper",
            Self::TrumpetWeathered => "minecraft:waxed_weathered_copper",
            Self::TrumpetOxidized => "minecraft:waxed_oxidized_copper",
            Self::Imitate(instrument) => instrument.block_resource(),
            Self::Other(_) => return None,
        })
    }

    /// returns the block under the note block for this instrument's sound.
    pub fn instrument_block(&self) -> Option<GenericBlockState> {
        if matches!(self, Self::Imitate(_)) {
            return None;
        }
        let block = self.block_resource()?;
        Some(GenericBlockState {
            name: Cow::Borrowed(block),
            properties: HashMap::new(),
        })
    }

    /// returns the mob head block for this instrument, if it is a mob head instrument.
    pub fn head_block(&self) -> Option<GenericBlockState> {
        if !matches!(self, Self::Imitate(_)) {
            return None;
        }
        let block = self.block_resource()?;
        Some(GenericBlockState {
            name: block.into(),
            properties: HashMap::new(),
        })
    }
}

impl ImitateInstrument {
    /// returns the minecraft instrument property string for note block state.
    pub fn note_property(self) -> &'static str {
        match self {
            Self::Creeper => "creeper",
            Self::Skeleton => "skeleton",
            Self::Dragon => "ender_dragon",
            Self::WitherSkeleton => "wither_skeleton",
            Self::Piglin => "piglin",
            Self::Zombie => "zombie",
            Self::CustomHead => "custom_head",
        }
    }

    /// returns the block resource name for this mob head.
    pub fn block_resource(self) -> &'static str {
        match self {
            Self::Creeper => "minecraft:creeper_head",
            Self::Skeleton => "minecraft:skeleton_skull",
            Self::Dragon => "minecraft:dragon_head",
            Self::WitherSkeleton => "minecraft:wither_skeleton_skull",
            Self::Piglin => "minecraft:piglin_head",
            Self::Zombie => "minecraft:zombie_head",
            Self::CustomHead => "minecraft:player_head",
        }
    }
}

// Helpers
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Component-wise maximum of two [`BlockPos`].
fn _component_max(a: BlockPos, b: BlockPos) -> BlockPos {
    BlockPos::new(a.x.max(b.x), a.y.max(b.y), a.z.max(b.z))
}

/// Note block, or fallback on None.
fn note_block<T: AsRef<Tone>>(
    note: Option<T>,
    fallback: fn() -> GenericBlockState,
) -> GenericBlockState {
    note.and_then(|t| t.as_ref().note_block_state())
        .unwrap_or_else(fallback)
}

fn inst_block<T: AsRef<Tone>>(
    note: Option<T>,
    fallback: fn() -> GenericBlockState,
) -> GenericBlockState {
    note.and_then(|t| t.as_ref().instrument_block_state())
        .unwrap_or_else(fallback)
}

fn air<B: BlockState>() -> B {
    BlockState::air()
}

fn chain_block() -> GenericBlockState {
    GenericBlockState {
        name: "minecraft:smooth_stone".into(),
        properties: Default::default(),
    }
}

fn floor_block() -> GenericBlockState {
    GenericBlockState {
        name: "minecraft:gray_stained_glass".into(),
        properties: Default::default(),
    }
}

fn redstone_wire() -> GenericBlockState {
    let properties = HashMap::from([
        ("power".into(), "0".into()),
        ("north".into(), "side".into()),
        ("south".into(), "side".into()),
        ("east".into(), "side".into()),
        ("west".into(), "side".into()),
    ]);
    GenericBlockState {
        name: "minecraft:redstone_wire".into(),
        properties,
    }
}

/// Repeater block with delay and facing.
fn repeater(
    delay: impl Into<Cow<'static, str>>,
    facing: impl Into<Cow<'static, str>>,
) -> GenericBlockState {
    GenericBlockState {
        name: "minecraft:repeater".into(),
        properties: HashMap::from([
            ("delay".into(), delay.into()),
            ("facing".into(), facing.into()),
            ("locked".into(), "false".into()),
            ("powered".into(), "false".into()),
        ]),
    }
}

/// Sticky piston block, not extended.
fn sticky_piston<T: Into<Cow<'static, str>>>(facing: T) -> GenericBlockState {
    GenericBlockState {
        name: "minecraft:sticky_piston".into(),
        properties: HashMap::from([
            ("facing".into(), facing.into()),
            ("extended".into(), "false".into()),
        ]),
    }
}

/// Redstone block.
fn redstone_block() -> GenericBlockState {
    GenericBlockState {
        name: "minecraft:redstone_block".into(),
        properties: Default::default(),
    }
}

/// Redstone torch with lit state and optional facing.
fn redstone_torch<T: Into<Cow<'static, str>>>(facing: Option<T>, lit: bool) -> GenericBlockState {
    let lit = if lit { "true" } else { "false" };
    let name = match facing.is_some() {
        true => "minecraft:redstone_wall_torch".into(),
        false => "minecraft:redstone_torch".into(),
    };
    let properties = match facing {
        Some(f) => From::from([("lit".into(), lit.into()), ("facing".into(), f.into())]),
        None => From::from([("lit".into(), lit.into())]),
    };
    GenericBlockState { name, properties }
}
