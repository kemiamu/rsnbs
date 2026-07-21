//! Generate Minecraft litematic projections from NBS songs.

use crate::note::{Instrument, Note, Tone};
use itertools::iproduct;
use mcdata::BlockState;
use mcdata::{GenericBlockState, util::BlockPos};
use rustmatica::{Litematic, Region};
use std::borrow::Cow;
use std::collections::HashMap;

mod compact;
mod linear;
pub use compact::*;
pub use linear::*;

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
        let mut region: Region<GenericBlockState> =
            Region::new("Note Block Track Schematic", BlockPos::new(0, 0, 0), size);

        for (y, z, x) in iproduct!(0..size.y, 0..size.z, 0..size.x) {
            let pos = BlockPos::new(x, y, z);
            region.set_block(pos, layout.get_block(pos));
        }
        region.as_litematic(description, author)
    }
}

/// A queryable projection layout.
pub trait Layout {
    /// Total size of the bounding box.
    fn size(&self) -> BlockPos;
    /// Block at the given world position.
    fn get_block(&self, pos: BlockPos) -> GenericBlockState;
}

// Floor
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

// Arranged
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Which spatial direction sub-layouts are placed along.
#[derive(Clone, Copy)]
pub enum Axis {
    /// East–west axis (X).
    Easting,
    /// Vertical axis (Y).
    Elevation,
    /// South–north axis (Z).
    Southing,
}

impl Axis {
    /// The unit vector for this axis.
    pub fn unit(self) -> BlockPos {
        match self {
            Axis::Easting => BlockPos::new(1, 0, 0),
            Axis::Elevation => BlockPos::new(0, 1, 0),
            Axis::Southing => BlockPos::new(0, 0, 1),
        }
    }
}

/// A layout wrapper that arranges sub-layouts along an [`Axis`].
pub struct Arranged<L: Layout> {
    bands: Vec<(L, BlockPos)>,
    size: BlockPos,
}

impl<L: Layout> Arranged<L> {
    pub fn new<I: IntoIterator<Item = L>>(layouts: I, axis: Axis, gap: u32) -> Self {
        let unit: BlockPos = axis.unit();
        let gap_vec: BlockPos = unit * gap as i32;
        let mut cursor: BlockPos = -gap_vec;
        let mut extent: BlockPos = BlockPos::new(0, 0, 0);

        let placed = layouts.into_iter().map(|layout| {
            let size: BlockPos = layout.size();
            let anchor: BlockPos = cursor + gap_vec;
            let dot: i32 = size.x * unit.x + size.y * unit.y + size.z * unit.z;
            cursor = anchor + unit * dot;
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

        let Some(idx) = self
            .bands
            .partition_point(|(_, a)| a.y <= pos.y && a.z <= pos.z && a.x <= pos.x)
            .checked_sub(1)
        else {
            return air();
        };

        let (layout, anchor) = &self.bands[idx];
        let local = BlockPos::new(pos.x - anchor.x, pos.y - anchor.y, pos.z - anchor.z);
        let size = layout.size();

        match (0..size.x).contains(&local.x)
            && (0..size.y).contains(&local.y)
            && (0..size.z).contains(&local.z)
        {
            true => layout.get_block(local),
            false => air(),
        }
    }
}

// Aligned
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Like [`Arranged`], but aligns sub-layouts by their far-edge on cross-axes.
pub struct EdgeArranged<L: Layout> {
    inner: Reverse<Arranged<Reverse<L>>>,
}

impl<L: Layout> EdgeArranged<L> {
    /// `align` is passed to both inner (per-sub-layout) and outer (whole) Reverse.
    pub fn new<I>(layouts: I, axis: Axis, gap: u32, align: BlockPos) -> Self
    where
        I: IntoIterator<Item = L>,
    {
        debug_assert!(align.x == 0 || align.x == 1);
        debug_assert!(align.y == 0 || align.y == 1);
        debug_assert!(align.z == 0 || align.z == 1);
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

// Reverse
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Mirror-reverse a layout along given axes. Block facing unchanged.
pub struct Reverse<L: Layout> {
    layout: L,
    sign: BlockPos,
}

impl<L: Layout> Reverse<L> {
    pub fn new(layout: L, sign: BlockPos) -> Self {
        debug_assert!(sign.x == 0 || sign.x == 1);
        debug_assert!(sign.y == 0 || sign.y == 1);
        debug_assert!(sign.z == 0 || sign.z == 1);
        Self { layout, sign }
    }
}

impl<L: Layout> Layout for Reverse<L> {
    fn size(&self) -> BlockPos {
        self.layout.size()
    }

    fn get_block(&self, pos: BlockPos) -> GenericBlockState {
        let size = self.layout.size();
        let sign = self.sign;
        let orig = BlockPos::new(
            pos.x + sign.x * (size.x - 1 - 2 * pos.x),
            pos.y + sign.y * (size.y - 1 - 2 * pos.y),
            pos.z + sign.z * (size.z - 1 - 2 * pos.z),
        );
        self.layout.get_block(orig)
    }
}

// Block state projection methods
//
// ++++++++++++============++++++++++++============++++++++++++============

impl Tone {
    /// returns the minecraft note block block state for this tone.
    pub fn note_block_state(&self) -> Option<GenericBlockState> {
        let note = self.key().minecraft_note()?;
        let instr = self.instrument().instrument_property();
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
}

impl Instrument {
    /// Minecraft render data, indexed parallel to Instrument::NBS_INDEX.
    const INSTRUMENT_MAP: &[(&'static str, &'static str)] = &[
        ("harp", "minecraft:dirt"),
        ("bass", "minecraft:oak_planks"),
        ("basedrum", "minecraft:stone"),
        ("snare", "minecraft:sand"),
        ("hat", "minecraft:glass"),
        ("guitar", "minecraft:white_wool"),
        ("flute", "minecraft:clay"),
        ("bell", "minecraft:gold_block"),
        ("chime", "minecraft:packed_ice"),
        ("xylophone", "minecraft:bone_block"),
        ("iron_xylophone", "minecraft:iron_block"),
        ("cow_bell", "minecraft:soul_sand"),
        ("didgeridoo", "minecraft:pumpkin"),
        ("bit", "minecraft:emerald_block"),
        ("banjo", "minecraft:hay_block"),
        ("pling", "minecraft:glowstone"),
        ("trumpet", "minecraft:waxed_copper_block"),
        ("trumpet_exposed", "minecraft:waxed_exposed_copper"),
        ("trumpet_weathered", "minecraft:waxed_weathered_copper"),
        ("trumpet_oxidized", "minecraft:waxed_oxidized_copper"),
        ("creeper", "minecraft:creeper_head"),
        ("skeleton", "minecraft:skeleton_skull"),
        ("ender_dragon", "minecraft:dragon_head"),
        ("wither_skeleton", "minecraft:wither_skeleton_skull"),
        ("piglin", "minecraft:piglin_head"),
        ("zombie", "minecraft:zombie_head"),
        ("custom_head", "minecraft:player_head"),
    ];

    /// returns the instrument property string for minecraft note block state.
    pub fn instrument_property(&self) -> &'static str {
        let idx = u8::from(*self) as usize;
        Self::INSTRUMENT_MAP
            .get(idx)
            .map(|(prop, _)| *prop)
            .unwrap_or("custom")
    }

    /// returns the block resource name for this instrument.
    pub fn block_resource(&self) -> Option<&'static str> {
        let idx = u8::from(*self) as usize;
        Self::INSTRUMENT_MAP.get(idx).map(|(_, block)| *block)
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

// Helpers
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Note block, or fallback on None.
fn note_block<'a, N>(note: N, fallback: fn() -> GenericBlockState) -> GenericBlockState
where
    N: Into<Option<&'a Note>>,
{
    note.into()
        .and_then(|n| n.tone().note_block_state())
        .unwrap_or_else(fallback)
}

fn instrument_block<'a, N>(note: N, fallback: fn() -> GenericBlockState) -> GenericBlockState
where
    N: Into<Option<&'a Note>>,
{
    note.into()
        .and_then(|n| n.tone().instrument().instrument_block())
        .unwrap_or_else(fallback)
}

fn air<B>() -> B
where
    B: BlockState,
{
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
fn sticky_piston(facing: impl Into<Cow<'static, str>>) -> GenericBlockState {
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
