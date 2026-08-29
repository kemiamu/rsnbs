//! Linear time-proportional layout for NBS song projection.

use super::{EvenlyArranged, Facing, Layout};
use super::{WithFloor, air, chain_block, inst_block, note_block};
use super::{redstone_block, repeater, sticky_piston};
use crate::note::Tone;
use crate::schematic::{WireConn, wire_state};
use crate::types::{Tick, TimeAnchor};
use mcdata::{GenericBlockState, util::BlockPos};
use std::num::NonZero;
use std::vec::IntoIter as VecIter;

// MultiLinearLayout
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Multi-line linear noteblocks layout.
pub struct MultiLinearLayout(EvenlyArranged<LinearLayout>);

impl MultiLinearLayout {
    /// Create a linear layout from per-track notes.
    pub fn new<Trks, Trk, A, T>(tracks: Trks, gap: u32, song_length: Tick) -> Self
    where
        Trks: IntoIterator<Item = Trk>,
        Trk: IntoIterator<Item = (A, T)>,
        A: TimeAnchor,
        T: Into<Tone>,
        for<'a> &'a Trks: IntoIterator<Item = &'a Trk>,
        for<'a> &'a Trk: IntoIterator<Item = (&'a A, &'a T)>,
    {
        let scale = ScaleMode::from_tracks(&tracks);
        let layouts = tracks
            .into_iter()
            .flat_map(|notes| LinearLayout::new(notes, scale, song_length, None, 0));
        let pitch = BlockPos::new(scale.width() + gap as i32, 0, 0);
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

// StackedLinearLayout
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Multi-track linear layout stacked vertically, each with a floor platform below.
pub struct StackedLinearLayout(EvenlyArranged<WithFloor<LinearLayout>>);

impl StackedLinearLayout {
    /// Create a stacked linear layout from per-track notes.
    pub fn new<Trks, Trk, A, T>(
        tracks: Trks,
        wrap_length: Option<NonZero<Tick>>,
        gap: u32,
        full: bool,
        song_length: Tick,
    ) -> Self
    where
        Trks: IntoIterator<Item = Trk>,
        Trk: IntoIterator<Item = (A, T)>,
        A: TimeAnchor,
        T: Into<Tone>,
        for<'a> &'a Trks: IntoIterator<Item = &'a Trk>,
        for<'a> &'a Trk: IntoIterator<Item = (&'a A, &'a T)>,
    {
        let scale = ScaleMode::from_tracks(&tracks);
        let layouts = tracks.into_iter().flat_map(|notes| {
            LinearLayout::new(notes, scale, song_length, wrap_length, gap)
                .into_iter()
                .map(|layout| WithFloor::new(layout, full))
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

// LinearLayout
//
// ++++++++++++============++++++++++++============++++++++++++============

/// A zigzag linear layout for one track.
pub struct LinearLayout(EvenlyArranged<Row>);

impl LinearLayout {
    /// Builds lanes from one track's note events via the cell container.
    pub fn new<Trk, A, T>(
        notes: Trk,
        scale: ScaleMode,
        song_length: Tick,
        wrap_length: Option<NonZero<Tick>>,
        gap: u32,
    ) -> Vec<Self>
    where
        Trk: IntoIterator<Item = (A, T)>,
        A: TimeAnchor,
        T: Into<Tone>,
    {
        let min_cells = song_length
            .checked_sub(1)
            .map_or(0, |tick| scale.cell_slot(tick).0 + 1);
        let mut cells = Cells::new(notes, scale, min_cells);
        let width = scale.width() + gap as i32 + 1;
        let row_length = wrap_length.map_or(cells.len(), |w| w.get() as usize);

        let mut lanes: Vec<Vec<Row>> = Vec::new();
        while cells.has_notes() {
            let lane = (0..cells.len()).step_by(row_length).map(|start| {
                let index = start / row_length;
                let region = cells.window(start, row_length);
                Row::new(region, scale, width, index > 0, index % 2 == 0)
            });
            lanes.push(lane.collect());
        }
        lanes
            .into_iter()
            .map(|rows| Self(EvenlyArranged::new(rows, BlockPos::new(width - 2, 0, 0))))
            .collect()
    }
}

impl Layout for LinearLayout {
    fn block_at(&self, pos: BlockPos) -> Option<GenericBlockState> {
        self.0.get_block(pos)
    }

    fn size(&self) -> BlockPos {
        self.0.size()
    }
}

// Cells
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Cell-domain note container: the time stream is filed here once, and
/// every lane consumes cells by capacity afterwards.
pub(crate) struct Cells {
    slots: Vec<Cell>,
}

impl Cells {
    /// Files notes into their cells; `min` keeps silent cells inside the
    /// song length alive.
    fn new<Trk, A, T>(notes: Trk, scale: ScaleMode, min: usize) -> Self
    where
        Trk: IntoIterator<Item = (A, T)>,
        A: TimeAnchor,
        T: Into<Tone>,
    {
        let mut slots: Vec<(Vec<Tone>, Vec<Tone>)> = vec![Default::default(); min];
        for (anchor, note) in notes {
            let (cell, is_branch) = scale.cell_slot(anchor.into_tick());
            slots.resize_with(slots.len().max(cell + 1), Default::default);
            let (main, branch) = &mut slots[cell];
            match is_branch {
                true => branch.push(note.into()),
                false => main.push(note.into()),
            }
        }
        let slots = slots.into_iter().map(|(main, branch)| Cell {
            main: main.into_iter(),
            branch: branch.into_iter(),
        });
        let slots = slots.collect();
        Self { slots }
    }

    /// Number of cells, including trailing silent ones.
    fn len(&self) -> usize {
        self.slots.len()
    }

    /// The `len` cells starting at `start`, clamped to what exists.
    fn window(&mut self, start: usize, len: usize) -> &mut [Cell] {
        let end = start.saturating_add(len).min(self.slots.len());
        &mut self.slots[start..end]
    }

    /// Whether the container still holds unconsumed notes.
    fn has_notes(&self) -> bool {
        self.slots
            .iter()
            .any(|cell| cell.main.len() + cell.branch.len() > 0)
    }
}

/// One cell's note queues, main and branch.
pub(crate) struct Cell {
    main: VecIter<Tone>,
    branch: VecIter<Tone>,
}

impl Cell {
    /// Takes up to `cap` main notes out of the cell.
    fn take_main(&mut self, cap: usize) -> Vec<Tone> {
        self.main.by_ref().take(cap).collect()
    }

    /// Takes up to `cap` branch notes out of the cell.
    fn take_branch(&mut self, cap: usize) -> Vec<Tone> {
        self.branch.by_ref().take(cap).collect()
    }
}

// Row & Turn
//
// ++++++++++++============++++++++++++============++++++++++++============

/// One directional row of template cells.
struct Row {
    cells: EvenlyArranged<Template>,
    leading_turn: bool,
    south_bound: bool,
    size: BlockPos,
}

impl Row {
    /// Arranges its region of cells in the row direction, draining them.
    pub(crate) fn new(
        cells: &mut [Cell],
        scale: ScaleMode,
        width: i32,
        leading_turn: bool,
        south_bound: bool,
    ) -> Self {
        let templates: Vec<Template> = cells
            .iter_mut()
            .map(|cell| Template::new(cell, scale, south_bound))
            .collect();
        let depth = 2 * templates.len() as i32 + 2;
        let pitch = BlockPos::new(0, 0, if south_bound { 2 } else { -2 });
        let cells = EvenlyArranged::new(templates, pitch);
        let size = BlockPos::new(width, cells.size().y, depth);
        Self {
            cells,
            leading_turn,
            south_bound,
            size,
        }
    }
}

impl Layout for Row {
    fn block_at(&self, pos: BlockPos) -> Option<GenericBlockState> {
        use self::WireConn::*;
        let inner = self.cells.size();
        let offset = match self.south_bound {
            true => BlockPos::new(inner.x - self.size.x, 0, -1),
            false => BlockPos::new(inner.x - self.size.x, 0, self.size.z - 1 - inner.z),
        };
        let turn_z = if self.south_bound { 0 } else { self.size.z - 1 };
        let turning = self.leading_turn && pos.z == turn_z;
        match (turning, pos.y, self.size.x - pos.x, self.south_bound) {
            (true, 1, 2, true) => Some(wire_state(Side, None, None, Side, "0")),
            (true, 1, 2, false) => Some(wire_state(Side, None, Side, None, "0")),
            (true, 1, 2.., _) => Some(wire_state(Side, Side, None, None, "0")),
            (true, 0, 2.., _) => Some(chain_block()),
            (true, _, _, _) => Option::None,
            (false, _, _, _) => self.cells.try_get_block(pos + offset),
        }
    }

    fn size(&self) -> BlockPos {
        self.size
    }
}

// Template
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Template cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Template {
    main: [Option<Tone>; 2],
    branch: Branch,
    scale: ScaleMode,
    south_bound: bool,
}

/// Notes outside the two fixed main-line slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Branch {
    /// The third main-line note.
    Unbranched(Option<Tone>),
    /// The two branch-line notes.
    Branched([Option<Tone>; 2]),
}

impl Template {
    /// Takes one cell out of the region.
    ///
    /// Consumes up to three main and two branch notes; notes beyond the
    /// capacity stay in the cell, where the next lane's template picks
    /// them up.
    fn new(cell: &mut Cell, scale: ScaleMode, south_bound: bool) -> Self {
        let main = cell.take_main(3);
        let branch = cell.take_branch(2);
        Self::from_notes(scale, main, branch, south_bound)
    }

    /// Builds a cell from raw notes; `None` slots stay silent.
    fn from_notes<M, B>(scale: ScaleMode, main: M, branch: B, south_bound: bool) -> Self
    where
        M: IntoIterator<Item = Tone>,
        B: IntoIterator<Item = Tone>,
    {
        let (mut main_notes, mut branch_notes) = (main.into_iter(), branch.into_iter());
        let main = [main_notes.next(), main_notes.next()];
        let branch = match branch_notes.next() {
            Some(first) => Branch::Branched([Some(first), branch_notes.next()]),
            None => Branch::Unbranched(main_notes.next()),
        };

        Self {
            main,
            branch,
            scale,
            south_bound,
        }
    }
}

impl Layout for Template {
    fn block_at(&self, pos: BlockPos) -> Option<GenericBlockState> {
        use self::{Branch::*, Facing::*, ScaleMode::*};
        let local_z = if self.south_bound { pos.z } else { 2 - pos.z };
        let local_x = self.scale.width() - pos.x - 1;
        let facing = if self.south_bound { South } else { North };
        let main_repeater = || repeater(self.scale.scale().to_string(), facing, false, false);
        let branch_repeater = || repeater((self.scale.scale() / 2).to_string(), West, false, false);
        match (self.scale, self.branch, local_x, pos.y, local_z) {
            (_any_scale, _any_branch, 1, 0, 0) => Some(chain_block()),
            (_any_scale, _any_branch, 1, 1, 0) => Some(main_repeater()),
            (_any_scale, _any_branch, 1, 0, 1) => Some(inst_block(self.main[0], chain_block)),
            (_any_scale, _any_branch, 1, 1, 1) => Some(note_block(self.main[0], chain_block)),
            (_any_scale, _any_branch, 0, 0, 1) => Some(inst_block(self.main[1], air)),
            (_any_scale, _any_branch, 0, 1, 1) => Some(note_block(self.main[1], air)),
            (_any_scale, Unbranched(note), 2, 0, 1) => Some(inst_block(note, air)),
            (_any_scale, Unbranched(note), 2, 1, 1) => Some(note_block(note, air)),

            (Scale4 | Scale2, Branched(_), 2, 0, 1) => Some(chain_block()),
            (Scale4 | Scale2, Branched(_), 2, 1, 1) => Some(branch_repeater()),
            (Scale4 | Scale2, Branched(b), 3, 0, 1) => Some(inst_block(b[0], chain_block)),
            (Scale4 | Scale2, Branched(b), 3, 1, 1) => Some(note_block(b[0], chain_block)),
            (Scale4 | Scale2, Branched(b), 4, 0, 1) => Some(inst_block(b[1], air)),
            (Scale4 | Scale2, Branched(b), 4, 1, 1) => Some(note_block(b[1], air)),

            (Scale3 | Scale1, Branched(_), 2, 1, 1) => Some(sticky_piston("west")),
            (Scale3 | Scale1, Branched(_), 3, 1, 1) => Some(redstone_block()),
            (Scale3 | Scale1, Branched(b), 5, 0, 1) => Some(inst_block(b[0], air)),
            (Scale3 | Scale1, Branched(b), 5, 1, 1) => Some(note_block(b[0], air)),
            (Scale3 | Scale1, Branched(b), 4, 0, 2) => Some(inst_block(b[1], air)),
            (Scale3 | Scale1, Branched(b), 4, 1, 2) => Some(note_block(b[1], air)),

            _ => None,
        }
    }

    fn size(&self) -> BlockPos {
        BlockPos::new(self.scale.width(), 2, 3)
    }
}

// ScaleMode
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Time scale used by linear cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScaleMode {
    Scale4,
    Scale2,
    Scale3,
    Scale1,
}

impl ScaleMode {
    const SCALE_MODES: [Tick; 3] = [4, 3, 2];

    /// Selects the coarsest scale compatible with every event timestamp.
    pub fn from_tracks<'a, Trks, Trk: 'a, A: 'a, T: 'a>(tracks: &'a Trks) -> Self
    where
        &'a Trks: IntoIterator<Item = &'a Trk>,
        &'a Trk: IntoIterator<Item = (&'a A, &'a T)>,
        A: TimeAnchor,
    {
        let ticks = tracks
            .into_iter()
            .flat_map(|track| track.into_iter().map(|(anchor, _)| (*anchor).into_tick()));
        Self::new(ticks)
    }

    /// Selects the coarsest scale compatible with all timestamps.
    ///
    /// The fallback is [`ScaleMode::Scale1`].
    pub fn new<I: IntoIterator<Item = Tick>>(ticks: I) -> Self {
        let applicable = ticks.into_iter().fold([true; 3], |applicable, tick| {
            let divisible = Self::SCALE_MODES.map(|scale| tick % scale == 0);
            std::array::from_fn(|index| applicable[index] && divisible[index])
        });
        match applicable.iter().position(|&is_applicable| is_applicable) {
            Some(0) => Self::Scale4,
            Some(1) => Self::Scale3,
            Some(2) => Self::Scale2,
            _ => Self::Scale1,
        }
    }

    /// Returns the scale in game ticks.
    pub const fn scale(self) -> Tick {
        match self {
            Self::Scale4 => 4,
            Self::Scale2 => 2,
            Self::Scale3 => 3,
            Self::Scale1 => 1,
        }
    }

    /// Returns the cell width in blocks.
    pub const fn width(self) -> i32 {
        match self {
            Self::Scale4 | Self::Scale2 => 5,
            Self::Scale3 | Self::Scale1 => 6,
        }
    }

    pub fn cell_slot(self, tick: Tick) -> (usize, bool) {
        let tick = tick / self.scale();
        let branch = tick % 2 == 1;
        let cell = tick as usize / 2 + usize::from(self == Self::Scale1 && !branch);
        (cell, branch)
    }
}
