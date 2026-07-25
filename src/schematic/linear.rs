//! Linear time-proportional layout for NBS song projection.

use super::{Arranged, Axis, Layout, WithFloor, air, chain_block, inst_block, note_block};
use super::{redstone_block, redstone_wire, repeater, sticky_piston};
use crate::note::Tone;
use crate::types::{Index, IntoTick, Position, Tick};
use mcdata::{GenericBlockState, util::BlockPos};

use std::collections::BTreeMap;
use std::num::NonZero;

//  MultiLinearLayout
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Multi-line linear noteblocks layout.
pub struct MultiLinearLayout(Arranged<LinearLayout>);

impl MultiLinearLayout {
    /// Create a linear layout from per-track notes.
    pub fn new<I, N, T>(tracks: I, gap: u32) -> Self
    where
        I: IntoIterator<Item = N>,
        N: IntoIterator<Item = (Position, T)>,
        T: Into<Tone>,
        for<'a> &'a I: IntoIterator<Item = &'a N>,
        for<'a> &'a N: IntoIterator<Item = (&'a Position, &'a T)>,
    {
        let meta = Meta::new(&tracks);
        let layouts = tracks
            .into_iter()
            .map(|notes| LinearLayout::new(notes, meta, None, 0));
        Self(Arranged::new(layouts, Axis::Easting, gap))
    }
}

impl Layout for MultiLinearLayout {
    fn size(&self) -> BlockPos {
        self.0.size()
    }

    fn get_block(&self, pos: BlockPos) -> GenericBlockState {
        self.0.get_block(pos)
    }
}

//  StackedLinearLayout
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Multi-track linear layout stacked vertically, each with a floor platform below.
pub struct StackedLinearLayout(Arranged<WithFloor<LinearLayout>>);

impl StackedLinearLayout {
    /// Create a stacked linear layout from per-track notes.
    pub fn new<I, N, T>(tracks: I, wrap_length: Option<NonZero<Tick>>, gap: u32, full: bool) -> Self
    where
        I: IntoIterator<Item = N>,
        N: IntoIterator<Item = (Position, T)>,
        T: Into<Tone>,
        for<'a> &'a I: IntoIterator<Item = &'a N>,
        for<'a> &'a N: IntoIterator<Item = (&'a Position, &'a T)>,
    {
        let meta = Meta::new(&tracks);
        let layouts = tracks.into_iter().map(|notes| {
            let layout = LinearLayout::new(notes, meta, wrap_length, gap);
            WithFloor::new(layout, full)
        });
        Self(Arranged::new(layouts, Axis::Elevation, 1))
    }
}

impl Layout for StackedLinearLayout {
    fn size(&self) -> BlockPos {
        self.0.size()
    }

    fn get_block(&self, pos: BlockPos) -> GenericBlockState {
        self.0.get_block(pos)
    }
}

// LinearLayoutMeta
//
// ++++++++++++============++++++++++++============++++++++++++============

type Meta = LinearLayoutMeta;

/// Metadata for constructing a [`LinearLayout`], derived from a tick stream.
#[derive(Clone, Copy)]
pub struct LinearLayoutMeta {
    song_length: Tick,
    scale: Tick,
}

impl LinearLayoutMeta {
    /// Compute metadata from a stream of tick positions.
    pub fn new<'a, I, N: 'a, T: 'a>(tracks: &'a I) -> Self
    where
        &'a I: IntoIterator<Item = &'a N>,
        &'a N: IntoIterator<Item = (&'a Position, &'a T)>,
    {
        let found_scale = Track::TEMPL.into_iter().find(|&templ| {
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
    track: Track,
    easting: i32,
    southing: i32,
}

impl LinearLayout {
    pub fn new<N, T>(notes: N, meta: Meta, wrap_length: Option<NonZero<Tick>>, gap: u32) -> Self
    where
        N: IntoIterator<Item = (Position, T)>,
        T: Into<Tone>,
    {
        let track = Track::new(notes, meta, wrap_length, gap);
        let easting = (track.width() + gap as i32) * track.wrap_rows() as i32 - gap as i32 + 1;
        let southing = track.cols_per_row() as i32 * Track::SOUTHING + 2;
        Self {
            track,
            easting,
            southing,
        }
    }

    fn turn_block(idx: i32) -> GenericBlockState {
        match idx {
            0 => chain_block(),
            1 => redstone_wire(),
            _ => air(),
        }
    }
}

impl Layout for LinearLayout {
    fn size(&self) -> BlockPos {
        BlockPos::new(self.easting, Track::ELEVATION, self.southing)
    }

    fn get_block(&self, pos: BlockPos) -> GenericBlockState {
        let width = self.track.width();
        let pitch = width + self.track.gap as i32;
        let gap = self.track.gap as i32;
        let BlockPos { x, y, z } = pos;

        // Turn blocks at front (0) and back (self.southing-1) zigzag edges
        let at_turn = |x: i32| (x + 1 + gap).rem_euclid(pitch * 2) <= pitch;
        if z == 0 && at_turn(x) || z + 1 == self.southing && at_turn(x + pitch) {
            return Self::turn_block(y);
        }

        let mut cell_x = x.rem_euclid(pitch);
        let mut cell = x.div_euclid(pitch);
        let interior = z > 0 && z + 1 < self.southing;
        let overlap = cell_x < 1;
        let zig = (cell + z).rem_euclid(2);

        // Inline relative offset
        let forward = match overlap {
            true => z.rem_euclid(2) == 0,
            false => cell.rem_euclid(2) == 0,
        };
        let offset = if forward { z } else { self.southing - 1 - z };

        // overlapping region
        if overlap && interior && zig == 0 {
            let col = (offset - 1).div_euclid(Track::SOUTHING);
            let local_pos = BlockPos::new(0, y, 1);
            return self.track.get_block(cell, col, local_pos);
        } else if overlap {
            cell_x += width + gap;
            cell -= 1;
        }

        // Not in overlapping region
        if cell_x == 1 && offset >= 2 {
            let col = (offset - 2).div_euclid(Track::SOUTHING);
            let local_z = (offset - 2).rem_euclid(Track::SOUTHING) + 1;
            let local_pos = BlockPos::new(1, y, local_z);
            return self.track.get_block(cell, col, local_pos);
        } else if interior && cell_x > 1 {
            let col = (offset - 1).div_euclid(Track::SOUTHING);
            let local_z = (offset - 1).rem_euclid(Track::SOUTHING);
            let local_pos = BlockPos::new(cell_x, y, local_z);
            return self.track.get_block(cell, col, local_pos);
        }

        air()
    }
}

// Track
//
// ++++++++++++============++++++++++++============++++++++++++============

struct Track {
    notes: BTreeMap<Position, Tone>,
    meta: Meta,
    wrap_length: Option<NonZero<Tick>>,
    gap: u32,
}

impl Track {
    pub const SOUTHING: i32 = 2;
    pub const ELEVATION: i32 = 2;
    pub const TEMPL: [Tick; 3] = [4, 2, 3];

    pub fn new<N, T>(notes: N, meta: Meta, wrap_length: Option<NonZero<Tick>>, gap: u32) -> Self
    where
        N: IntoIterator<Item = (Position, T)>,
        T: Into<Tone>,
    {
        let notes = notes.into_iter().map(|(pos, t)| (pos, t.into())).collect();

        Self {
            notes,
            meta,
            wrap_length,
            gap,
        }
    }

    fn get_block(&self, row: i32, col: i32, local_pos: BlockPos) -> GenericBlockState {
        let BlockPos {
            x: easting,
            y: elevation,
            z: southing,
        } = local_pos;

        let is_piston = self.is_piston();
        let scale = self.meta.scale;
        let branch_tick = if is_piston { 3 } else { scale };

        let easting = match is_piston || easting < 2 {
            true => easting,
            false => easting + 1,
        };
        let repeater_facing = match row.rem_euclid(2) == 0 {
            true => "north",
            false => "south",
        };
        let note = move |tick: Tick, layer: Index| -> Option<&Tone> {
            let group = row * self.cols_per_row() as i32 + col;
            let head = if scale == 1 { 1 } else { 0 };
            let base_tick = (group - head) * scale as i32 * 2;
            let tick = (tick as i32 + base_tick).try_into().ok()?;
            self.notes.get(&Position::new(tick, layer))
        };
        let has_branch = note(branch_tick, 0)
            .or_else(|| note(branch_tick, 1))
            .is_some();

        match (has_branch, is_piston, easting, southing, elevation) {
            (true, true, 3, 1, 1) => sticky_piston("west"),
            (true, true, 2, 1, 1) => redstone_block(),
            (true, true, 1, 2, 0) => inst_block(note(branch_tick, 0), air),
            (true, true, 1, 2, 1) => note_block(note(branch_tick, 0), air),
            (true, true, 0, 1, 0) => inst_block(note(branch_tick, 1), air),
            (true, true, 0, 1, 1) => note_block(note(branch_tick, 1), air),

            (true, false, 3, 1, 0) => chain_block(),
            (true, false, 3, 1, 1) => repeater((scale / 2).to_string(), "east"),
            (true, false, 1, 1, 0) => inst_block(note(branch_tick, 0), chain_block),
            (true, false, 1, 1, 1) => note_block(note(branch_tick, 0), chain_block),
            (true, false, 0, 1, 0) => inst_block(note(branch_tick, 1), air),
            (true, false, 0, 1, 1) => note_block(note(branch_tick, 1), air),

            (_, _, 4, 0, 0) => chain_block(),
            (_, _, 4, 0, 1) => repeater(scale.to_string(), repeater_facing),
            (_, _, 4, 1, 0) => inst_block(note(0, 0), chain_block),
            (_, _, 4, 1, 1) => note_block(note(0, 0), chain_block),
            (_, _, 5, 1, 0) => inst_block(note(0, 1), air),
            (_, _, 5, 1, 1) => note_block(note(0, 1), air),
            (false, _, 3, 1, 0) => inst_block(note(0, 2), air),
            (false, _, 3, 1, 1) => note_block(note(0, 2), air),

            _ => air(),
        }
    }

    fn is_piston(&self) -> bool {
        match self.meta.scale {
            4 | 2 => false,
            3 | 1 => true,
            _ => unreachable!(),
        }
    }

    fn width(&self) -> i32 {
        if self.is_piston() { 5 } else { 4 }
    }

    fn length_in_units(&self, multiplier: NonZero<Tick>) -> Tick {
        let head = if self.meta.scale == 1 { 2 } else { 0 };
        (self.meta.song_length + head).div_ceil(self.meta.scale * 2 * multiplier.get())
    }

    fn wrap_rows(&self) -> Tick {
        self.wrap_length
            .map_or(1, |wrap| self.length_in_units(wrap))
    }

    fn cols_per_row(&self) -> Tick {
        let all_cols = self.length_in_units(NonZero::<Tick>::MIN);
        self.wrap_length.map_or(all_cols, |wrap| wrap.get())
    }
}
