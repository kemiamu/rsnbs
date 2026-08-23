//! Generate Minecraft litematic projections from NBS songs.

pub use self::blocks::*;
pub use self::compact::*;
pub use self::linear::*;
pub use self::tapped::*;
use itertools::iproduct;
use mcdata::{BlockState, GenericBlockState, util::BlockPos};
use rustmatica::{Litematic, Region};
use std::borrow::Cow;

mod blocks;
mod compact;
mod linear;
mod tapped;

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
            extent = include(extent, size);
            (layout, anchor)
        });

        let bands = placed.collect();
        let size = include(include(cursor, BlockPos::ORIGIN), extent);
        Self { bands, size }
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
            extent = include(extent, far);
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
    /// When `false`, only positions with a gravity block above get a floor.
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
            false if self.layout.get_block(pos).needs_floor() => floor_block(),
            false => air(),
        };
        let local_pos = || BlockPos::new(pos.x, pos.y - 1, pos.z);

        match pos.y {
            0 => floor(),
            _ => self.layout.get_block(local_pos()),
        }
    }
}

/// Whether a block state is a gravity block that needs floor support.
pub trait NeedsFloor {
    /// Whether this block state needs a floor to hold it up.
    fn needs_floor(&self) -> bool;
}

impl NeedsFloor for GenericBlockState {
    fn needs_floor(&self) -> bool {
        self.name == "minecraft:sand"
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

// Helpers
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Component-wise maximum of two [`BlockPos`].
fn include(a: BlockPos, b: BlockPos) -> BlockPos {
    BlockPos::new(a.x.max(b.x), a.y.max(b.y), a.z.max(b.z))
}

fn air<B: BlockState>() -> B {
    BlockState::air()
}

fn floor_block() -> GenericBlockState {
    GenericBlockState {
        name: "minecraft:gray_stained_glass".into(),
        properties: Default::default(),
    }
}
