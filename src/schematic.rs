//! Generate Minecraft litematic projections from NBS songs.

use crate::note::Note;
use itertools::iproduct;
use mcdata::BlockState;
use mcdata::{GenericBlockState, util::BlockPos};
use rustmatica::{Litematic, Region};
use std::borrow::Cow;
use std::collections::HashMap;

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
            let block = layout.get_block(pos).unwrap_or_else(air);
            region.set_block(pos, block);
        }
        region.as_litematic(description, author)
    }
}

/// A queryable projection layout.
pub trait Layout {
    /// Total size of the bounding box.
    fn size(&self) -> BlockPos;
    /// Block at the given world position.
    fn get_block(&self, pos: BlockPos) -> Option<GenericBlockState>;
}

// Floor
//
// ++++++++++++============++++++++++++============++++++++++++============

/// A layout wrapper that adds a floor layer beneath another layout.
pub struct WithFloor<L: Layout>(pub L);

impl<L: Layout> Layout for WithFloor<L> {
    fn size(&self) -> BlockPos {
        let size = self.0.size();
        BlockPos::new(size.x, size.y + 1, size.z)
    }

    fn get_block(&self, pos: BlockPos) -> Option<GenericBlockState> {
        match pos.y {
            0 => Some(floor_block()),
            _ => self.0.get_block(BlockPos::new(pos.x, pos.y - 1, pos.z)),
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
///
/// Sub-layouts are placed one after another along the chosen axis,
/// with the cross-axes sized to the maximum among all children.
/// Always aligns toward 0.
pub struct Arranged<L: Layout> {
    bands: Vec<(L, BlockPos)>,
    size: BlockPos,
}

impl<L: Layout> Arranged<L> {
    pub fn new(layouts: impl IntoIterator<Item = L>, axis: Axis, gap: u32) -> Self {
        let unit = axis.unit();
        let gap = gap as i32;
        let gap_vec = BlockPos::new(unit.x * gap, unit.y * gap, unit.z * gap);
        let mut cursor = BlockPos::new(-gap_vec.x, -gap_vec.y, -gap_vec.z);
        let mut extent = BlockPos::new(0, 0, 0);

        let placed = layouts.into_iter().map(|layout| {
            let size = layout.size();
            // Anchor along the primary axis (cross-axes stay 0)
            let anchor_pos = BlockPos::new(
                cursor.x + gap_vec.x,
                cursor.y + gap_vec.y,
                cursor.z + gap_vec.z,
            );
            // Advance cursor along the primary axis
            cursor = BlockPos::new(
                anchor_pos.x + size.x * unit.x,
                anchor_pos.y + size.y * unit.y,
                anchor_pos.z + size.z * unit.z,
            );
            // Accumulate max across all axes
            extent = BlockPos::new(
                extent.x.max(size.x),
                extent.y.max(size.y),
                extent.z.max(size.z),
            );
            (layout, anchor_pos)
        });
        let bands = placed.collect();

        // max picks primary from cursor, cross from extent
        let size = BlockPos::new(
            cursor.x.max(0).max(extent.x),
            cursor.y.max(0).max(extent.y),
            cursor.z.max(0).max(extent.z),
        );
        Self { bands, size }
    }
}

impl<L: Layout> Layout for Arranged<L> {
    fn size(&self) -> BlockPos {
        self.size
    }

    fn get_block(&self, pos: BlockPos) -> Option<GenericBlockState> {
        debug_assert!((0..self.size.x).contains(&pos.x), "x out of range");
        debug_assert!((0..self.size.y).contains(&pos.y), "y out of range");
        debug_assert!((0..self.size.z).contains(&pos.z), "z out of range");

        let idx = self
            .bands
            .partition_point(|(_, a)| (a.y, a.z, a.x) <= (pos.y, pos.z, pos.x))
            .checked_sub(1)?;
        let (layout, anchor) = &self.bands[idx];
        let local = BlockPos::new(pos.x - anchor.x, pos.y - anchor.y, pos.z - anchor.z);
        let size = layout.size();
        match (0..size.x).contains(&local.x)
            && (0..size.y).contains(&local.y)
            && (0..size.z).contains(&local.z)
        {
            true => layout.get_block(local),
            false => None,
        }
    }
}

// Helpers
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Air block (no block).
pub fn air<B>() -> B
where
    B: BlockState,
{
    BlockState::air()
}

/// A chain block (smooth stone) used for structural support.
pub fn chain_block() -> GenericBlockState {
    GenericBlockState {
        name: "minecraft:smooth_stone".into(),
        properties: Default::default(),
    }
}

/// Floor block.
pub fn floor_block() -> GenericBlockState {
    GenericBlockState {
        name: "minecraft:white_concrete".into(),
        properties: Default::default(),
    }
}

/// A redstone wire block with all-side connections.
pub fn redstone_wire() -> GenericBlockState {
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
pub fn repeater(
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

/// Note block, or fallback on None.
pub fn note_block<'a, N>(note: N, fallback: fn() -> GenericBlockState) -> GenericBlockState
where
    N: Into<Option<&'a Note>>,
{
    note.into()
        .and_then(|n| n.note_block_state())
        .unwrap_or_else(fallback)
}

/// Instrument block, or fallback on None.
pub fn instrument_block<'a, N>(note: N, fallback: fn() -> GenericBlockState) -> GenericBlockState
where
    N: Into<Option<&'a Note>>,
{
    note.into()
        .and_then(|n| n.instrument.instrument_block())
        .unwrap_or_else(fallback)
}

/// Sticky piston block, not extended.
pub fn sticky_piston(facing: impl Into<Cow<'static, str>>) -> GenericBlockState {
    GenericBlockState {
        name: "minecraft:sticky_piston".into(),
        properties: HashMap::from([
            ("facing".into(), facing.into()),
            ("extended".into(), "false".into()),
        ]),
    }
}

/// Redstone block.
pub fn redstone_block() -> GenericBlockState {
    GenericBlockState {
        name: "minecraft:redstone_block".into(),
        properties: Default::default(),
    }
}
