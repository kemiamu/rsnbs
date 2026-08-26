//! Linear time-proportional layout for NBS song projection.

use super::{Anchored, Arranged, Axis, Clipped, EvenlyArranged, Facing, Layout, Mask, Reverse};
use super::{WithFloor, air, chain_block, inst_block, note_block};
use super::{redstone_block, redstone_wire, repeater, sticky_piston};
use crate::note::Tone;
use crate::types::{Index, LayerAnchor, Position, Tick, TimeAnchor};
use mcdata::{GenericBlockState, util::BlockPos};
use std::num::NonZero;

//  MultiLinearLayout
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Multi-line linear noteblocks layout.
pub struct MultiLinearLayout(EvenlyArranged<LinearLayout>);

impl MultiLinearLayout {
    /// Create a linear layout from per-track notes.
    pub fn new<Trks, Trk, T>(tracks: Trks, gap: u32) -> Self
    where
        Trks: IntoIterator<Item = Trk>,
        Trk: IntoIterator<Item = (Position, T)>,
        T: Into<Tone>,
        for<'a> &'a Trks: IntoIterator<Item = &'a Trk>,
        for<'a> &'a Trk: IntoIterator<Item = (&'a Position, &'a T)>,
    {
        let meta = Meta::new(&tracks);
        let layouts = tracks
            .into_iter()
            .map(|notes| LinearLayout::new(notes, meta, None, 0));
        let pitch = BlockPos::new(LinearLayout::cell(meta.scale) + gap as i32, 0, 0);
        Self(EvenlyArranged::new(layouts, pitch))
    }
}

impl Layout for MultiLinearLayout {
    fn block_at(&self, pos: BlockPos) -> Option<GenericBlockState> {
        self.0.get_block(pos)
    }

    fn size(&self) -> BlockPos {
        self.0.size()
    }
}

//  StackedLinearLayout
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Multi-track linear layout stacked vertically, each with a floor platform below.
pub struct StackedLinearLayout(EvenlyArranged<WithFloor<LinearLayout>>);

impl StackedLinearLayout {
    /// Create a stacked linear layout from per-track notes.
    pub fn new<Trks, Trk, T>(
        tracks: Trks,
        wrap_length: Option<NonZero<Tick>>,
        gap: u32,
        full: bool,
    ) -> Self
    where
        Trks: IntoIterator<Item = Trk>,
        Trk: IntoIterator<Item = (Position, T)>,
        T: Into<Tone>,
        for<'a> &'a Trks: IntoIterator<Item = &'a Trk>,
        for<'a> &'a Trk: IntoIterator<Item = (&'a Position, &'a T)>,
    {
        let meta = Meta::new(&tracks);
        let layouts = tracks.into_iter().map(|notes| {
            let layout = LinearLayout::new(notes, meta, wrap_length, gap);
            WithFloor::new(layout, full)
        });
        let pitch = BlockPos::new(0, 4, 0);
        Self(EvenlyArranged::new(layouts, pitch))
    }
}

impl Layout for StackedLinearLayout {
    fn block_at(&self, pos: BlockPos) -> Option<GenericBlockState> {
        self.0.get_block(pos)
    }

    fn size(&self) -> BlockPos {
        self.0.size()
    }
}

// LinearLayoutMeta
//
// ++++++++++++============++++++++++++============++++++++++++============

type Meta = LinearLayoutMeta;

/// Metadata for constructing a [`LinearLayout`], derived from a tick stream.
#[derive(Clone, Copy)]
pub struct LinearLayoutMeta {
    pub song_length: Tick,
    pub scale: Tick,
}

impl LinearLayoutMeta {
    /// Compute metadata from a stream of tick positions.
    pub fn new<'a, Trks, Trk: 'a, T: 'a>(tracks: &'a Trks) -> Self
    where
        &'a Trks: IntoIterator<Item = &'a Trk>,
        &'a Trk: IntoIterator<Item = (&'a Position, &'a T)>,
    {
        let found_scale = TEMPL.into_iter().find(|&templ| {
            tracks
                .into_iter()
                .flat_map(|n| n.into_iter().map(|(pos, _)| pos.into_tick()))
                .all(|t| t % templ == 0)
        });
        let scale = found_scale.unwrap_or(1);
        let song_length = tracks
            .into_iter()
            .flat_map(|n| n.into_iter().map(|(pos, _)| pos.into_tick()))
            .max()
            .map_or(0, |t| t + 1);
        Self { song_length, scale }
    }
}

// LinearLayout
//
// ++++++++++++============++++++++++++============++++++++++++============

/// A single linear track layout.
pub struct LinearLayout {
    inner: Clipped<EvenlyArranged<Row>>,
}

impl LinearLayout {
    pub fn new<Trk, T>(notes: Trk, meta: Meta, wrap_length: Option<NonZero<Tick>>, gap: u32) -> Self
    where
        Trk: IntoIterator<Item = (Position, T)>,
        T: Into<Tone>,
    {
        todo!()
    }
}

impl Layout for LinearLayout {
    fn block_at(&self, pos: BlockPos) -> Option<GenericBlockState> {
        self.inner.get_block(pos)
    }

    fn size(&self) -> BlockPos {
        self.inner.size()
    }
}

// Row
//
// ++++++++++++============++++++++++++============++++++++++++============

/// A complete zigzag row. The cell stream contains only data-bearing
/// templates; turn junctions are owned and created by the row itself.
pub struct Row {
    inner: Anchored,
}

impl Row {
    /// Arrange a template stream and add all structure owned by that row.
    ///
    /// Direction is data on the templates' repeaters, so the row needs no
    /// global row index or mode flag. A northbound row emits its main-column
    /// overhang into the following row's overlap band.
    pub fn new<I: IntoIterator<Item = Template>>(cells: I, gap: i32) -> Self {
        todo!()
    }
}

impl Layout for Row {
    fn block_at(&self, pos: BlockPos) -> Option<GenericBlockState> {
        self.inner.get_block(pos)
    }

    fn size(&self) -> BlockPos {
        self.inner.size()
    }
}

// Track sizing
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Candidate track scales, tried in order for divisibility.
const TEMPL: [Tick; 3] = [4, 2, 3];

fn length_in_units(meta: &Meta, multiplier: NonZero<Tick>) -> Tick {
    let head = if meta.scale == 1 { 2 } else { 0 };
    (meta.song_length + head).div_ceil(meta.scale * 2 * multiplier.get())
}

fn wrap_rows(meta: &Meta, wrap_length: Option<NonZero<Tick>>) -> i32 {
    wrap_length.map_or(1, |wrap| length_in_units(meta, wrap)) as i32
}

fn cols_per_row(meta: &Meta, wrap_length: Option<NonZero<Tick>>) -> i32 {
    let all = length_in_units(meta, NonZero::<Tick>::MIN);
    wrap_length.map_or(all, |wrap| wrap.get()) as i32
}

// Template
//
// ++++++++++++============++++++++++++============++++++++++++============

/// A fixed-shape template tile of the linear track, including orientation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Template {
    /// Repeater branch cell: logic column at x3, branch notes at x0/x1,
    /// main notes at x4/x5. Size 6×2×2.
    Branch {
        repeater: Repeater,
        branch: [Option<Tone>; 2],
        notes: [Option<Tone>; 2],
    },
    /// Piston branch cell: piston column at x2/x3, branch notes at x0/x1,
    /// main notes at x4/x5. Size 6×2×2.
    Piston {
        repeater: Repeater,
        branch: [Option<Tone>; 2],
        notes: [Option<Tone>; 2],
    },
    /// Note cell: spare note at x3, main notes at x4/x5. Size 6×2×2.
    Note {
        repeater: Repeater,
        notes: [Option<Tone>; 3],
    },
}

impl Layout for Template {
    fn block_at(&self, pos: BlockPos) -> Option<GenericBlockState> {
        todo!()
    }

    fn size(&self) -> BlockPos {
        match self {
            Self::Piston { .. } => BlockPos::new(6, 2, 2),
            Self::Branch { .. } | Self::Note { .. } => BlockPos::new(5, 2, 2),
        }
    }
}

// Repeater
//
// ++++++++++++============++++++++++++============++++++++++++============

/// A repeater tile: delay and facing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Repeater {
    /// Signal delay: the track scale.
    delay: u8,
    /// Facing of this repeater.
    facing: Facing,
}

impl Repeater {
    /// Signal repeater block: full delay, data facing.
    fn block(self) -> GenericBlockState {
        repeater(self.delay.to_string(), self.facing, false, false)
    }

    /// Logic repeater block: half delay, fixed east facing.
    fn logic_block(self) -> GenericBlockState {
        repeater((self.delay / 2).to_string(), Facing::West, false, false)
    }
}
