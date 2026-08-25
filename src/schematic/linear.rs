//! Linear time-proportional layout for NBS song projection.

use super::{Anchored, Arranged, Axis, Clipped, EvenlyArranged, Facing, Layout, Mask, Reverse};
use super::{WithFloor, air, chain_block, inst_block, note_block};
use super::{redstone_block, redstone_wire, repeater, sticky_piston};
use crate::note::Tone;
use crate::types::{Index, Position, Tick, TimeAnchor};
use mcdata::{GenericBlockState, util::BlockPos};
use std::collections::BTreeMap;
use std::iter;
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
///
/// The track is built at construction time into zigzag rows: each row is a
/// column-stack of template cells plus its turn junctions, and rows are then
/// evenly arranged along the track axis with overlapping overhangs absorbed
/// by [`EvenlyArranged`]'s candidate fallback.
pub struct LinearLayout {
    inner: Clipped<EvenlyArranged<Anchored>>,
}

impl LinearLayout {
    /// Width of one cell column: pistons widen the branch band.
    fn column(scale: Tick) -> i32 {
        if matches!(scale, 1 | 3) { 5 } else { 4 }
    }

    /// Row width of a track: the cell column plus its trailing turn block.
    pub fn cell(scale: Tick) -> i32 {
        Self::column(scale) + 1
    }

    /// X-extent of a track built from `meta`.
    pub fn easting(meta: Meta, wrap_length: Option<NonZero<Tick>>, gap: u32) -> i32 {
        (Self::column(meta.scale) + gap as i32) * wrap_rows(&meta, wrap_length) + 1 - gap as i32
    }

    pub fn new<Trk, T>(notes: Trk, meta: Meta, wrap_length: Option<NonZero<Tick>>, gap: u32) -> Self
    where
        Trk: IntoIterator<Item = (Position, T)>,
        T: Into<Tone>,
    {
        let notes: BTreeMap<Position, Tone> =
            notes.into_iter().map(|(pos, t)| (pos, t.into())).collect();
        let is_piston = matches!(meta.scale, 1 | 3);
        let width = Self::column(meta.scale);
        let pitch = width + gap as i32;
        let cols = cols_per_row(&meta, wrap_length);
        let rows = wrap_rows(&meta, wrap_length);
        let southing = cols * 2 + 2;

        // With no gap the last x band reaches `rows * pitch`, so a phantom
        // row beyond the physical ones supplies the notes it exposes.
        let physical = rows;
        let row_count = physical + (gap == 0) as i32;
        let rows: Vec<Anchored> = (0..row_count)
            .map(|row| {
                build_row(
                    &notes, meta, is_piston, row, cols, southing, pitch, gap as i32,
                )
            })
            .collect();

        let arranged = EvenlyArranged::new(rows, BlockPos::new(pitch, 0, 0));
        // Trim the trailing cell overhang down to the legacy easting.
        let easting = Self::easting(meta, wrap_length, gap);
        let size = arranged.size();
        let inner = Clipped::new(arranged, BlockPos::new(easting - size.x, 0, 0));
        Self { inner }
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

// Row construction
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Build one zigzag row: turn strips on both edges, the previous row's main
/// column overhang at the head, and the column stack of template cells.
fn build_row(
    notes: &BTreeMap<Position, Tone>,
    meta: Meta,
    is_piston: bool,
    row: i32,
    cols: i32,
    southing: i32,
    pitch: i32,
    gap: i32,
) -> Anchored {
    let scale = meta.scale as i32;
    let head = if scale == 1 { 1 } else { 0 };
    let branch_tick = if is_piston { 3 } else { scale };
    let note = |tick: i32, layer: Index, group: i32| -> Option<Tone> {
        let base_tick = (group - head) * scale * 2;
        let tick = (tick + base_tick).try_into().ok()?;
        notes.get(&Position::new(tick, layer)).copied()
    };

    // Column stack of cells; odd rows run the columns in reverse.
    let cells: Vec<Template> = (0..cols)
        .map(|col| {
            let group = row * cols + col;
            let branch = [note(branch_tick, 0, group), note(branch_tick, 1, group)];
            let has_branch = branch[0].or(branch[1]).is_some();
            let repeater = Repeater {
                delay: meta.scale as u8,
                facing: match row.rem_euclid(2) {
                    0 => Facing::South,
                    _ => Facing::North,
                },
            };
            let notes = [note(0, 0, group), note(0, 1, group)];
            match (has_branch, is_piston) {
                (true, true) => Template::Piston {
                    repeater,
                    branch,
                    notes,
                },
                (true, false) => Template::Branch {
                    repeater,
                    branch,
                    notes,
                },
                (false, _) => Template::Note {
                    repeater,
                    notes: [notes[0], notes[1], note(0, 2, group)],
                },
            }
        })
        .collect();
    let columns = Arranged::new(cells, Axis::Southing, 0);
    let columns: Box<dyn Layout> = match row.rem_euclid(2) {
        0 => Box::new(columns),
        _ => Box::new(Reverse::new(
            columns,
            Mask::new(BlockPos::new(0, 0, 1)).unwrap(),
        )),
    };
    // Non-piston tracks skip the piston-only x=2 band: queries at x >= 2
    // shift one band right, collapsing the gap.
    let cells: Box<dyn Layout> = if is_piston {
        columns
    } else {
        Box::new(NonPistonShift(columns))
    };

    // Piston branch[0] overhangs into the next column's z0 band (z odd on
    // even rows, z even on odd rows), so it is a separate overhang column;
    // the plain branch[0] lives inside the cell at its z1 band instead.
    let branch0: Box<dyn Layout> = if is_piston {
        let columns = (0..cols).map(|col| {
            let column = match row.rem_euclid(2) {
                0 => col,
                _ => cols - 1 - col,
            };
            let group = row * cols + column;
            let has_branch = note(branch_tick, 0, group)
                .or(note(branch_tick, 1, group))
                .is_some();
            let b0 = note(branch_tick, 0, group);
            ExtendCol {
                under: has_branch.then(|| inst_block(b0, air)),
                note: has_branch.then(|| note_block(b0, air)),
            }
        });
        Box::new(EvenlyArranged::new(columns, BlockPos::new(0, 0, 2)))
    } else {
        Box::new(Anchored::new(
            std::iter::empty::<(Box<dyn Layout>, BlockPos)>(),
        ))
    };

    // Overhang of the previous row's main column into this row's overlap
    // zone. In the zigzag, even rows expose the previous (reversed) row's
    // n1 track at every other z column; odd rows expose nothing there.
    let extend: Box<dyn Layout> = match (row.rem_euclid(2), gap) {
        // With no gap the overlap band reads the previous row's n1 track;
        // with a gap that band resolves past the track's far edge to air.
        (0, 0) => {
            let columns = (0..cols).map(|col| {
                let column = cols - 1 - col;
                let group = (row - 1) * cols + column;
                ExtendCol {
                    under: Some(inst_block(note(0, 1, group), air)),
                    note: Some(note_block(note(0, 1, group), air)),
                }
            });
            Box::new(EvenlyArranged::new(columns, BlockPos::new(0, 0, 2)))
        }
        _ => Box::new(Anchored::new(
            std::iter::empty::<(Box<dyn Layout>, BlockPos)>(),
        )),
    };

    // Turn junction strips: even rows carry the front strip at the row head,
    // odd rows at the row tail, with the two edge bands complementary.
    let front = pitch - gap - 1;
    let (lo0, hi0, lo1, hi1) = match row.rem_euclid(2) {
        0 => (0, front, front, pitch - 1),
        _ => (front, pitch - 1, 0, front),
    };
    let strip0: Box<dyn Layout> = Box::new(Arranged::new(
        iter::repeat(Template::Turn).take((hi0 - lo0 + 1) as usize),
        Axis::Easting,
        0,
    ));
    let strip1: Box<dyn Layout> = Box::new(Arranged::new(
        iter::repeat(Template::Turn).take((hi1 - lo1 + 1) as usize),
        Axis::Easting,
        0,
    ));

    Anchored::new(vec![
        (strip0, BlockPos::new(lo0, 0, 0)),
        (strip1, BlockPos::new(lo1, 0, southing - 1)),
        (extend, BlockPos::new(0, 0, 1 + row.rem_euclid(2))),
        // Even rows overhang into the odd z band; odd rows into the even z
        // band from the row head (z = 0) with reversed column order.
        (
            branch0,
            BlockPos::new(
                1,
                0,
                match row.rem_euclid(2) {
                    0 => 3,
                    _ => 0,
                },
            ),
        ),
        (cells, BlockPos::new(0, 0, 1)),
    ])
}

// NonPistonShift
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Collapses the piston-only x=2 band out of a non-piston cell stack:
/// queries at `x >= 2` shift one band right, so the logic/spare, main and
/// n1 bands sit at x 2..5 instead of the piston's 3..5.
struct NonPistonShift<L: Layout>(L);

impl<L: Layout> Layout for NonPistonShift<L> {
    fn block_at(&self, pos: BlockPos) -> Option<GenericBlockState> {
        let x = if pos.x >= 2 { pos.x + 1 } else { pos.x };
        self.0.get_block(BlockPos::new(x, pos.y, pos.z))
    }

    fn size(&self) -> BlockPos {
        let size = self.0.size();
        BlockPos::new(size.x - 1, size.y, size.z)
    }
}

// ExtendCol
//
// ++++++++++++============++++++++++++============++++++++++++============

/// A single-column overhang tile with an optional under/note pair,
/// used for the previous row's n1 track and the piston branch[0] band.
/// `None` under/note means the band is empty at that column.
struct ExtendCol {
    under: Option<GenericBlockState>,
    note: Option<GenericBlockState>,
}

impl Layout for ExtendCol {
    fn block_at(&self, pos: BlockPos) -> Option<GenericBlockState> {
        match pos.y {
            0 => self.under.clone(),
            1 => self.note.clone(),
            _ => None,
        }
    }

    fn size(&self) -> BlockPos {
        BlockPos::new(1, 2, 1)
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
///
/// Each variant is a complete structural unit resolved by local coordinates;
/// note data is baked in at construction time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Template {
    /// Turn junction column: chain at y0, wire at y1. Size 1×2×1.
    Turn,
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

impl Layout for Template {
    fn block_at(&self, pos: BlockPos) -> Option<GenericBlockState> {
        let BlockPos { x, y, z } = pos;
        let block = match self {
            Template::Turn => match y {
                0 => chain_block(),
                1 => redstone_wire(),
                _ => return None,
            },
            Template::Branch {
                repeater,
                branch,
                notes,
            } => match (x, y, z) {
                (0, 0, 1) => inst_block(branch[1], air),
                (0, 1, 1) => note_block(branch[1], air),
                (1, 0, 1) => inst_block(branch[0], chain_block),
                (1, 1, 1) => note_block(branch[0], chain_block),
                (3, 0, 1) => chain_block(),
                (3, 1, 1) => repeater.logic_block(),
                (4, 0, 0) => chain_block(),
                (4, 1, 0) => repeater.block(),
                (4, 0, 1) => inst_block(notes[0], chain_block),
                (4, 1, 1) => note_block(notes[0], chain_block),
                (5, 0, 1) => inst_block(notes[1], air),
                (5, 1, 1) => note_block(notes[1], air),
                _ => return None,
            },
            Template::Piston {
                repeater,
                branch,
                notes,
            } => match (x, y, z) {
                (0, 0, 1) => inst_block(branch[1], air),
                (0, 1, 1) => note_block(branch[1], air),
                (2, 1, 1) => redstone_block(),
                (3, 1, 1) => sticky_piston("west"),
                (4, 0, 0) => chain_block(),
                (4, 1, 0) => repeater.block(),
                (4, 0, 1) => inst_block(notes[0], chain_block),
                (4, 1, 1) => note_block(notes[0], chain_block),
                (5, 0, 1) => inst_block(notes[1], air),
                (5, 1, 1) => note_block(notes[1], air),
                _ => return None,
            },
            Template::Note { repeater, notes } => match (x, y, z) {
                (3, 0, 1) => inst_block(notes[2], air),
                (3, 1, 1) => note_block(notes[2], air),
                (4, 0, 0) => chain_block(),
                (4, 1, 0) => repeater.block(),
                (4, 0, 1) => inst_block(notes[0], chain_block),
                (4, 1, 1) => note_block(notes[0], chain_block),
                (5, 0, 1) => inst_block(notes[1], air),
                (5, 1, 1) => note_block(notes[1], air),
                _ => return None,
            },
        };
        Some(block)
    }

    fn size(&self) -> BlockPos {
        match self {
            Template::Turn => BlockPos::new(1, 2, 1),
            Template::Branch { .. } | Template::Piston { .. } | Template::Note { .. } => {
                BlockPos::new(6, 2, 2)
            }
        }
    }
}
