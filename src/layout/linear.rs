//! Linear time-proportional layout for NBS song projection.

use crate::note::Notes;
use crate::schematic::{Arranged, Axis, Layout, air, chain_block, instrument_block};
use crate::schematic::{note_block, redstone_block, redstone_wire, repeater, sticky_piston};
use crate::types::{Index, Position, Tick};
use mcdata::{GenericBlockState, util::BlockPos};
use std::num::NonZero;

//  MultiLinearLayout
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Multi-line linear noteblocks layout.
pub struct MultiLinearLayout(Arranged<LinearLayout>);

impl MultiLinearLayout {
    /// Create a linear layout from per-track notes.
    pub fn new(tracks: Vec<Notes>, gap: u32) -> Self {
        let positions = tracks.iter().flat_map(|n| n.keys());
        let factor = Track::TEMPL
            .into_iter()
            .find(|&templ| positions.clone().all(|pos| pos.tick() % templ == 0))
            .unwrap_or(1);
        let song_length = tracks
            .iter()
            .flat_map(|n| n.keys().map(|pos| pos.tick()))
            .max()
            .map_or(0, |t| t + 1);
        let layouts = tracks
            .into_iter()
            .map(|notes| LinearLayout::new(notes, song_length, factor, NonZero::new(16), gap));
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
    pub fn new(
        notes: Notes,
        song_length: Tick,
        scale: Tick,
        wrap_length: Option<NonZero<Tick>>,
        gap: u32,
    ) -> Self {
        let track = Track {
            notes,
            song_length,
            scale,
            wrap_length,
            gap,
        };
        let easting = (((track.width() as u32 + gap) * track.wrap_rows()) - gap).max(0) as i32 + 1;
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
        if (pos.z == 0
            && (pos.x + 1 + self.track.gap as i32)
                .rem_euclid((self.track.width() + self.track.gap as i32) * 2)
                <= self.track.width() + self.track.gap as i32)
            || ((pos.z + 1) == self.southing
                && (pos.x
                    + (self.track.width() + self.track.gap as i32)
                    + 1
                    + self.track.gap as i32)
                    .rem_euclid((self.track.width() + self.track.gap as i32) * 2)
                    <= self.track.width() + self.track.gap as i32)
        {
            Self::turn_block(pos.y)
        } else if pos.z > 0
            && (pos.z + 1) < self.southing
            && pos.x.rem_euclid(self.track.width() + self.track.gap as i32) == 0
        {
            let cell = pos.x.div_euclid(self.track.width() + self.track.gap as i32);
            let zig = (cell + pos.z).rem_euclid(2);
            if zig == 1 && self.track.gap != 0 {
                return air();
            }
            let row = cell - zig;
            let col = match pos.z.rem_euclid(2) == 0 {
                true => (pos.z - 1).div_euclid(Track::SOUTHING),
                false => (self.southing - pos.z - 2).div_euclid(Track::SOUTHING),
            };
            let local_pos = match zig == 0 {
                true => BlockPos::new(0, pos.y, 1),
                false => BlockPos::new(self.track.width(), pos.y, 1),
            };
            self.track.get_block(row, col, local_pos)
        } else if pos.x.rem_euclid(self.track.width() + self.track.gap as i32) == 1 {
            let row = pos.x.div_euclid(self.track.width() + self.track.gap as i32);
            let col = match row.rem_euclid(2) == 0 {
                true => (pos.z - 2).div_euclid(Track::SOUTHING),
                false => (self.southing - pos.z - 3).div_euclid(Track::SOUTHING),
            };
            let local_pos = match row.rem_euclid(2) == 0 {
                true => BlockPos::new(1, pos.y, (pos.z - 2).rem_euclid(2) + 1),
                false => BlockPos::new(1, pos.y, (self.southing - pos.z - 3).rem_euclid(2) + 1),
            };
            self.track.get_block(row, col, local_pos)
        } else if pos.z > 0
            && (pos.z + 1) < self.southing
            && pos.x.rem_euclid(self.track.width() + self.track.gap as i32) >= 2
        {
            let row = pos.x.div_euclid(self.track.width() + self.track.gap as i32);
            let col = match row.rem_euclid(2) == 0 {
                true => (pos.z - 1).div_euclid(Track::SOUTHING),
                false => (self.southing - pos.z - 2).div_euclid(Track::SOUTHING),
            };
            let local_pos = match row.rem_euclid(2) == 0 {
                true => BlockPos::new(
                    pos.x - row * (self.track.width() + self.track.gap as i32),
                    pos.y,
                    (pos.z - 1).rem_euclid(2),
                ),
                false => BlockPos::new(
                    pos.x - row * (self.track.width() + self.track.gap as i32),
                    pos.y,
                    (self.southing - pos.z - 2).rem_euclid(2),
                ),
            };
            self.track.get_block(row, col, local_pos)
        } else {
            air()
        }
    }
}

// Track
//
// ++++++++++++============++++++++++++============++++++++++++============

struct Track {
    notes: Notes,
    song_length: Tick,
    scale: Tick,
    wrap_length: Option<NonZero<Tick>>,
    gap: u32,
}

impl Track {
    pub const SOUTHING: i32 = 2;
    pub const ELEVATION: i32 = 2;
    pub const TEMPL: [Tick; 3] = [4, 2, 3];

    fn is_piston(&self) -> bool {
        match self.scale {
            4 | 2 => false,
            3 | 1 => true,
            _ => unreachable!(),
        }
    }

    fn width(&self) -> i32 {
        if self.is_piston() { 5 } else { 4 }
    }

    fn length_in_units(&self, multiplier: Tick) -> Tick {
        self.song_length.div_ceil(self.scale * 2 * multiplier)
    }

    fn wrap_rows(&self) -> Tick {
        self.wrap_length
            .map_or(1, |wrap| self.length_in_units(wrap.get() as Tick))
    }

    fn cols_per_row(&self) -> Tick {
        self.wrap_length
            .map_or(self.length_in_units(1), |wrap| wrap.get() as Tick)
    }

    fn get_block(&self, row: i32, col: i32, local_pos: BlockPos) -> GenericBlockState {
        let BlockPos {
            x: easting,
            y: elevation,
            z: southing,
        } = local_pos;

        let repeater_facing = match row.rem_euclid(2) == 0 {
            true => "north",
            false => "south",
        };
        let is_piston = self.is_piston();
        let scale = self.scale;
        let branch_tick = if is_piston { 3 } else { scale };

        let easting = match !is_piston && easting > 1 {
            true => easting + 1,
            false => easting,
        };

        let note = move |tick: Tick, layer: Index| {
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
            (true, true, 1, 2, 0) => instrument_block(note(branch_tick, 0), air),
            (true, true, 1, 2, 1) => note_block(note(branch_tick, 0), air),
            (true, true, 0, 1, 0) => instrument_block(note(branch_tick, 1), air),
            (true, true, 0, 1, 1) => note_block(note(branch_tick, 1), air),

            (true, false, 3, 1, 0) => chain_block(),
            (true, false, 3, 1, 1) => repeater((scale / 2).to_string(), "east"),
            (true, false, 1, 1, 0) => instrument_block(note(branch_tick, 0), chain_block),
            (true, false, 1, 1, 1) => note_block(note(branch_tick, 0), chain_block),
            (true, false, 0, 1, 0) => instrument_block(note(branch_tick, 1), air),
            (true, false, 0, 1, 1) => note_block(note(branch_tick, 1), air),

            (_, _, 4, 0, 0) => chain_block(),
            (_, _, 4, 0, 1) => repeater(scale.to_string(), repeater_facing),
            (_, _, 4, 1, 0) => instrument_block(note(0, 0), chain_block),
            (_, _, 4, 1, 1) => note_block(note(0, 0), chain_block),
            (_, _, 5, 1, 0) => instrument_block(note(0, 1), air),
            (_, _, 5, 1, 1) => note_block(note(0, 1), air),
            (false, _, 3, 1, 0) => instrument_block(note(0, 2), air),
            (false, _, 3, 1, 1) => note_block(note(0, 2), air),

            _ => air(),
        }
    }
}
