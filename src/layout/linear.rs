//! Linear time-proportional layout for NBS song projection.

use crate::note::Notes;
use crate::schematic::{Arranged, Axis, Layout, air, chain_block, instrument_block, note_block};
use crate::schematic::{redstone_block, repeater, sticky_piston};
use crate::types::{Position, Tick};
use mcdata::{GenericBlockState, util::BlockPos};

//  MultiLinearLayout
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Multi-line linear noteblocks layout.
pub struct MultiLinearLayout(Arranged<LinearLayout>);

impl MultiLinearLayout {
    /// Create a linear layout from per-track notes.
    pub fn new(tracks: Vec<Notes>, gap: u32) -> Self {
        let positions = tracks.iter().flat_map(|n| n.keys());
        let factor = LinearLayout::TEMPL
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
            .map(|notes| LinearLayout::new(notes, song_length, factor));
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
    notes: Notes,
    scale: Tick,
    southing: i32,
}

impl LinearLayout {
    pub const SOUTHING: i32 = 2;
    pub const ELEVATION: i32 = 2;
    pub const TEMPL: [Tick; 3] = [4, 2, 3];

    pub fn new(notes: Notes, song_length: Tick, scale: Tick) -> Self {
        let southing = match scale {
            1 => (song_length.div_ceil(2) as i32) * Self::SOUTHING + Self::SOUTHING + 1,
            _ => (song_length.div_ceil(scale * 2) as i32) * Self::SOUTHING + 1,
        };
        Self {
            notes,
            scale,
            southing,
        }
    }

    fn is_piston(&self) -> bool {
        match self.scale {
            4 | 2 => false,
            3 | 1 => true,
            _ => unreachable!(),
        }
    }

    fn width(&self) -> i32 {
        if self.is_piston() { 6 } else { 5 }
    }

    fn track_block(&self, pos: BlockPos) -> GenericBlockState {
        let BlockPos {
            x: easting,
            y: elevation,
            z: southing,
        } = pos;

        let flat_southing = southing - if easting <= 2 { 0 } else { 1 };
        let local_southing = flat_southing.rem_euclid(Self::SOUTHING);
        let is_piston = self.is_piston();
        let scale = self.scale;

        let note = move |tick, layer| {
            let groups = flat_southing.div_euclid(Self::SOUTHING);
            let head = if scale == 1 { 1 } else { 0 };
            let base_tick = (groups - head) * scale as i32 * 2;
            let tick = (base_tick + tick as i32).try_into().ok()?;
            self.notes.get(&Position::new(tick, layer))
        };

        let branch_tick = if is_piston { 3 } else { scale };
        let has_branch = note(branch_tick, 0)
            .or_else(|| note(branch_tick, 1))
            .is_some();

        match (is_piston, has_branch, local_southing, easting, elevation) {
            (true, true, 1, 2, 1) => sticky_piston("east"),
            (true, true, 0, 3, 1) => redstone_block(),
            (true, true, 0, 5, 0) => instrument_block(note(3, 0), air),
            (true, true, 0, 5, 1) => note_block(note(3, 0), air),
            (true, true, 1, 4, 0) => instrument_block(note(3, 1), air),
            (true, true, 1, 4, 1) => note_block(note(3, 1), air),
            (false, true, 1, 2, 0) => chain_block(),
            (false, true, 1, 2, 1) => repeater((scale / 2).to_string(), "west"),
            (false, true, 0, 3, 0) => instrument_block(note(scale, 0), chain_block),
            (false, true, 0, 3, 1) => note_block(note(scale, 0), chain_block),
            (false, true, 0, 4, 0) => instrument_block(note(scale, 1), air),
            (false, true, 0, 4, 1) => note_block(note(scale, 1), air),
            (_, _, 0, 1, 0) => chain_block(),
            (_, _, 0, 1, 1) => repeater(scale.to_string(), "south"),
            (_, _, 1, 1, 0) => instrument_block(note(0, 0), chain_block),
            (_, _, 1, 1, 1) => note_block(note(0, 0), chain_block),
            (_, _, 1, 0, 0) => instrument_block(note(0, 1), air),
            (_, _, 1, 0, 1) => note_block(note(0, 1), air),
            (_, false, 1, 2, 0) => instrument_block(note(0, 2), air),
            (_, false, 1, 2, 1) => note_block(note(0, 2), air),
            _ => air(),
        }
    }
}

impl Layout for LinearLayout {
    fn size(&self) -> BlockPos {
        BlockPos::new(self.width(), Self::ELEVATION, self.southing)
    }

    fn get_block(&self, pos: BlockPos) -> GenericBlockState {
        let local = BlockPos::new(pos.x, pos.y, self.southing - pos.z - 1);
        self.track_block(local)
    }
}
