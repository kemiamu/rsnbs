//! Generate Minecraft litematic projections from NBS songs.

use itertools::iproduct;
use mcdata::{BlockState, GenericBlockState, util::BlockPos};
use rustmatica::{Litematic, Region};
use std::borrow::Cow;

pub use self::blocks::*;
pub use self::compact::*;
pub use self::linear::*;
pub use self::tapped::*;
mod blocks;
mod compact;
mod linear;
mod tapped;

// Layout trait
//
// ++++++++++++============++++++++++++============++++++++++++============

/// A queryable projection layout.
pub trait Layout {
    /// Block at the given world position, assumed to be in bounds.
    fn block_at(&self, pos: BlockPos) -> Option<GenericBlockState>;

    /// Total size of the bounding box.
    fn size(&self) -> BlockPos;

    /// Block at the given world position; panics on out-of-bounds access.
    fn get_block(&self, pos: BlockPos) -> Option<GenericBlockState> {
        debug_assert!(self.contains(pos), "block out of bounds");
        self.block_at(pos)
    }

    /// Block at the given world position; `None` when it misses the bounding box.
    fn try_get_block(&self, pos: BlockPos) -> Option<GenericBlockState> {
        match self.contains(pos) {
            true => self.block_at(pos),
            false => None,
        }
    }

    /// Whether `pos` is inside the bounding box.
    fn contains(&self, pos: BlockPos) -> bool {
        let size = self.size();
        (0..size.x).contains(&pos.x) && (0..size.y).contains(&pos.y) && (0..size.z).contains(&pos.z)
    }

    /// Build a litematic projection of this layout.
    fn as_litematic(
        &self,
        description: impl Into<Cow<'static, str>>,
        author: impl Into<Cow<'static, str>>,
    ) -> Litematic
    where
        Self: Sized,
    {
        const NAME: &str = "Note Block Track Schematic";
        let size = self.size();
        let mut region: Region<GenericBlockState> = Region::new(NAME, BlockPos::ORIGIN, size);

        for (y, z, x) in iproduct!(0..size.y, 0..size.z, 0..size.x) {
            let pos = BlockPos::new(x, y, z);
            region.set_block(pos, self.get_block(pos).unwrap_or_else(air));
        }
        region.as_litematic(description, author)
    }
}

impl Layout for Box<dyn Layout + '_> {
    fn block_at(&self, pos: BlockPos) -> Option<GenericBlockState> {
        self.as_ref().block_at(pos)
    }

    fn size(&self) -> BlockPos {
        self.as_ref().size()
    }
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
    fn block_at(&self, pos: BlockPos) -> Option<GenericBlockState> {
        self.inner.get_block(pos)
    }

    fn size(&self) -> BlockPos {
        self.inner.size()
    }
}

// Arranged
//
// ++++++++++++============++++++++++++============++++++++++++============

/// A layout wrapper that arranges sub-layouts along an [`Axis`].
///
/// Query cost is O(log n) with `n` the number of sub-layouts: [`Layout::block_at`]
/// locates the containing band via a binary search over the anchors, which are
/// sorted along the arrangement axis.
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
    fn block_at(&self, pos: BlockPos) -> Option<GenericBlockState> {
        let index = self
            .bands
            .partition_point(|(_, a)| a.y <= pos.y && a.z <= pos.z && a.x <= pos.x)
            .checked_sub(1)?;

        let (layout, anchor) = &self.bands[index];
        let local = BlockPos::new(pos.x - anchor.x, pos.y - anchor.y, pos.z - anchor.z);
        layout.try_get_block(local)
    }

    fn size(&self) -> BlockPos {
        self.size
    }
}

// EvenlyArranged
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Evenly arranges sub-layouts along the direction of `pitch`: item `i` is
/// anchored at `i * pitch`, a monotone linear lattice of equal spacing.
///
/// Query cost is O(k) with `k` the candidate window width, independent of
/// the item count. `pitch` must be component-wise non-negative and not all zero.
pub struct EvenlyArranged<L: Layout> {
    items: Vec<L>,
    pitch: BlockPos,
    spacing: i32,
    cell: i32,
    size: BlockPos,
}

impl<L: Layout> EvenlyArranged<L> {
    pub fn new<I: IntoIterator<Item = L>>(items: I, pitch: BlockPos) -> Self {
        let items: Vec<L> = FromIterator::from_iter(items);
        let extent = items
            .iter()
            .map(Layout::size)
            .fold(BlockPos::ORIGIN, include);
        let spacing = pitch.dot(pitch);
        let cell = pitch.dot(extent);
        let size = include(pitch * (items.len() as i32 - 1) + extent, BlockPos::ORIGIN);

        assert!(spacing > 0 && pitch == pitch.abs());
        Self {
            items,
            pitch,
            spacing,
            cell,
            size,
        }
    }
}

impl<L: Layout> Layout for EvenlyArranged<L> {
    fn block_at(&self, pos: BlockPos) -> Option<GenericBlockState> {
        let offset = self.pitch.dot(pos);
        let start = (offset / self.spacing).min(self.items.len() as i32 - 1);
        let low = 0.max((offset - self.cell).div_euclid(self.spacing) + 1);
        (low..=start)
            .rev()
            .find_map(|index| self.items[index as usize].try_get_block(pos - self.pitch * index))
    }

    fn size(&self) -> BlockPos {
        self.size
    }
}

// Anchored
//
// ++++++++++++============++++++++++++============++++++++++++============

/// A layout wrapper that places sub-layouts at explicit anchor positions.
///
/// Queries probe every entry in order via dynamic dispatch, so the cost is
/// O(n) in the entry count and comparatively heavy; suited to small fixed
/// groupings.
pub struct Anchored {
    entries: Vec<(Box<dyn Layout>, BlockPos)>,
    size: BlockPos,
}

impl Anchored {
    /// Build from initial entries; [`Self::push`] appends more later.
    pub fn new(entries: impl IntoIterator<Item = (Box<dyn Layout>, BlockPos)>) -> Self {
        let mut anchored = Self {
            entries: Vec::new(),
            size: BlockPos::ORIGIN,
        };
        for (layout, anchor) in entries {
            anchored.push(layout, anchor);
        }
        anchored
    }

    pub fn push(&mut self, layout: impl Into<Box<dyn Layout>>, anchor: BlockPos) {
        let layout = layout.into();
        let far = anchor + layout.size();
        self.size = include(self.size, far);
        self.entries.push((layout, anchor));
    }
}

impl Layout for Anchored {
    fn block_at(&self, pos: BlockPos) -> Option<GenericBlockState> {
        self.entries
            .iter()
            .find_map(|(layout, anchor)| layout.try_get_block(pos - *anchor))
    }

    fn size(&self) -> BlockPos {
        self.size
    }
}

// Overlap
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Overlaps two sub-layouts at the origin.
impl<A: Layout, B: Layout> Layout for (A, B) {
    fn block_at(&self, pos: BlockPos) -> Option<GenericBlockState> {
        self.0
            .try_get_block(pos)
            .or_else(|| self.1.try_get_block(pos))
    }

    fn size(&self) -> BlockPos {
        include(self.0.size(), self.1.size())
    }
}

// Reverse
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Mirror-reverse a layout along given axes. Block facing unchanged:
/// the layout has no block-internal facing transform, so the block's own
/// orientation state is never dictated by it.
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
    fn block_at(&self, pos: BlockPos) -> Option<GenericBlockState> {
        let size = self.layout.size();
        let orig = pos + self.sign * (size - BlockPos::new(1, 1, 1) - pos * 2);
        self.layout.get_block(orig)
    }

    fn size(&self) -> BlockPos {
        self.layout.size()
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
    fn block_at(&self, pos: BlockPos) -> Option<GenericBlockState> {
        let local_pos = || BlockPos::new(pos.x, pos.y - 1, pos.z);
        match pos.y {
            0 if self.full => Some(floor_block()),
            0 if self.layout.get_block(pos).is_some_and(|b| b.needs_floor()) => Some(floor_block()),
            _ => self.layout.get_block(local_pos()),
        }
    }

    fn size(&self) -> BlockPos {
        let size = self.layout.size();
        BlockPos::new(size.x, size.y + 1, size.z)
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

// Clipped
//
// ++++++++++++============++++++++++++============++++++++++++============

/// A layout wrapper that clips the inner layout along each axis by `pos`.
///
/// A non-negative component `n` shifts the inner layout by `n` and shrinks
/// the size by `n`; a negative component keeps the inner layout in place and
/// shrinks the size by `-n`.
pub struct Clipped<L: Layout> {
    layout: L,
    anchor: BlockPos,
    size: BlockPos,
}

impl<L: Layout> Clipped<L> {
    pub fn new(layout: L, pos: BlockPos) -> Self {
        let anchor = include(pos, BlockPos::ORIGIN);
        let size = layout.size() - pos.abs();

        Self {
            layout,
            anchor,
            size,
        }
    }
}

impl<L: Layout> Layout for Clipped<L> {
    fn block_at(&self, pos: BlockPos) -> Option<GenericBlockState> {
        self.layout.get_block(pos + self.anchor)
    }

    fn size(&self) -> BlockPos {
        self.size
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

    /// Unwrap into the underlying [`BlockPos`].
    pub fn into_inner(self) -> BlockPos {
        self.0
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

// Dot
//
// ++++++++++++============++++++++++++============++++++++++++============

/// The scalar projection of `v` onto `self`, treating both as vectors.
trait Dot {
    fn dot(self, v: BlockPos) -> i32;
}

impl Dot for BlockPos {
    fn dot(self, v: BlockPos) -> i32 {
        self.x * v.x + self.y * v.y + self.z * v.z
    }
}

impl Dot for Mask {
    fn dot(self, v: BlockPos) -> i32 {
        self.into_inner().dot(v)
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
