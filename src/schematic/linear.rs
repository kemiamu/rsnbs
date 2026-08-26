//! Linear time-proportional layout for NBS song projection.

use super::{Axis, EvenlyArranged, Facing, Layout, Reverse};
use super::{WithFloor, air, chain_block, inst_block, note_block};
use super::{redstone_block, repeater, sticky_piston};
use crate::note::Tone;
use crate::schematic::{WireConn, wire_state};
use crate::types::{Tick, TimeAnchor};
use mcdata::{GenericBlockState, util::BlockPos};
use std::{collections::VecDeque, num::NonZero};

// MultiLinearLayout
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Multi-line linear noteblocks layout.
pub struct MultiLinearLayout(EvenlyArranged<LinearLayout>);

impl MultiLinearLayout {
    /// Create a linear layout from per-track notes.
    pub fn new<Trks, Trk, A, T>(tracks: Trks, gap: u32) -> Self
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
            .map(|notes| LinearLayout::new(notes, scale, None, 0));
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
        let layouts = tracks.into_iter().map(|notes| {
            let layout = LinearLayout::new(notes, scale, wrap_length, gap);
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

// LinearLayout
//
// ++++++++++++============++++++++++++============++++++++++++============

pub struct LinearLayout(EvenlyArranged<Row>);

impl LinearLayout {
    pub fn new<Trk, A, T>(
        notes: Trk,
        scale: ScaleMode,
        wrap_length: Option<NonZero<Tick>>,
        gap: u32,
    ) -> Self
    where
        Trk: IntoIterator<Item = (A, T)>,
        A: TimeAnchor,
        T: Into<Tone>,
    {
        let mut cells: VecDeque<(Vec<Tone>, Vec<Tone>)> = Default::default();
        for (anchor, note) in notes {
            let (index, is_branch) = scale.cell_slot(anchor.into_tick());
            cells.resize_with(index + 1, Default::default);
            let (main, branch) = &mut cells[index];
            let notes = if is_branch { branch } else { main };
            notes.push(note.into());
        }

        let row_length = wrap_length.map_or(cells.len(), |length| length.get() as usize);
        let width = scale.width() + gap as i32;
        let mut rows = Vec::new();
        while !cells.is_empty() {
            let index = rows.len();
            let south_bound = index % 2 == 0;
            let templates = cells
                .drain(..row_length.min(cells.len()))
                .map(|(main, branch)| Template::from_notes(scale, main, branch, south_bound));
            rows.push(Row::new(templates, width, index > 0, south_bound));
        }
        Self(EvenlyArranged::new(rows, BlockPos::new(width - 2, 0, 0)))
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

// Row & Turn
//
// ++++++++++++============++++++++++++============++++++++++++============

pub struct Row {
    cells: Cells,
    width: i32,
    leading_turn: bool,
    south_bound: bool,
}

impl Row {
    pub fn new<I: IntoIterator<Item = Template>>(
        cells: I,
        width: i32,
        leading_turn: bool,
        south_bound: bool,
    ) -> Self {
        let cells = Cells::new(cells, south_bound);

        Self {
            cells,
            width,
            leading_turn,
            south_bound,
        }
    }
}

impl Layout for Row {
    fn block_at(&self, pos: BlockPos) -> Option<GenericBlockState> {
        use self::WireConn::*;
        let inner = self.cells.size();
        let offset = inner.x - self.width;
        let turning = self.leading_turn
            && ((self.south_bound && pos.z == 0) || (!self.south_bound && pos.z == inner.z));
        match (turning, pos.y, self.width - pos.x, self.south_bound) {
            (true, 1, 2, true) => Some(wire_state(Side, None, Side, None, "0")),
            (true, 1, 2, false) => Some(wire_state(Side, None, None, Side, "0")),
            (true, 1, 2.., _) => Some(wire_state(Side, Side, None, None, "0")),
            (true, 0, 2.., _) => Some(chain_block()),
            (true, _, _, _) => Option::None,

            (false, _, _, true) => self.cells.try_get_block(pos + BlockPos::new(offset, 0, 0)),
            (false, _, _, false) => self.cells.try_get_block(pos + BlockPos::new(offset, 0, 1)),
        }
    }

    fn size(&self) -> BlockPos {
        let inner = self.cells.size();
        BlockPos::new(self.width, inner.y, inner.z + 1)
    }
}

// Cells
//
// ++++++++++++============++++++++++++============++++++++++++============

enum Cells {
    South(EvenlyArranged<Template>),
    North(Reverse<EvenlyArranged<Reverse<Template>>>),
}

impl Cells {
    fn new<I: IntoIterator<Item = Template>>(cells: I, south_bound: bool) -> Self {
        let pitch = BlockPos::new(0, 0, 2);
        if south_bound {
            return Self::South(EvenlyArranged::new(cells, pitch));
        }
        let rev = cells
            .into_iter()
            .map(|cell| Reverse::new(cell, Axis::Southing.unit()));
        let cells = EvenlyArranged::new(rev, pitch);
        Self::North(Reverse::new(cells, Axis::Southing.unit()))
    }
}

impl Layout for Cells {
    fn block_at(&self, pos: BlockPos) -> Option<GenericBlockState> {
        match self {
            Self::South(cells) => cells.block_at(pos),
            Self::North(cells) => cells.block_at(pos),
        }
    }

    fn size(&self) -> BlockPos {
        match self {
            Self::South(cells) => cells.size(),
            Self::North(cells) => cells.size(),
        }
    }
}

// Template
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Template cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Template {
    pub main: [Option<Tone>; 2],
    pub branch: Branch,
    pub scale: ScaleMode,
    pub south_bound: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Branch {
    Unbranched(Option<Tone>),
    Branched([Option<Tone>; 2]),
}

impl Template {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScaleMode {
    Scale4,
    Scale2,
    Scale3,
    Scale1,
}

impl ScaleMode {
    const SCALE_MODES: [Tick; 3] = [4, 3, 2];

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

    /// Select the first applicable scaling mode.
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

    pub const fn scale(self) -> Tick {
        match self {
            Self::Scale4 => 4,
            Self::Scale2 => 2,
            Self::Scale3 => 3,
            Self::Scale1 => 1,
        }
    }

    pub const fn width(self) -> i32 {
        match self {
            Self::Scale4 | Self::Scale2 => 5,
            Self::Scale3 | Self::Scale1 => 6,
        }
    }

    fn cell_slot(self, tick: Tick) -> (usize, bool) {
        let tick = tick / self.scale();
        let branch = tick % 2 == 1;
        let cell = tick as usize / 2 + usize::from(self == Self::Scale1 && !branch);
        (cell, branch)
    }
}
