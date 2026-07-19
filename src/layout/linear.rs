//! Linear time-proportional layout for NBS song projection.

use crate::note::Notes;
use crate::schematic::{Arranged, Axis, Layout, air, chain_block, instrument_block, note_block};
use crate::schematic::{redstone_block, repeater, sticky_piston};
use crate::types::{Position, Tick};
use mcdata::{GenericBlockState, util::BlockPos};

//  LinearLayout
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Linear noteblocks layout.
pub struct LinearLayout(Arranged<SingleLinearLayout>);

impl LinearLayout {
    const TEMPL: [Tick; 3] = [4, 2, 3];

    /// Create a linear layout from per-track notes.
    pub fn new(tracks: Vec<Notes>, gap: u32) -> Self {
        let positions = tracks.iter().flat_map(|n| n.keys());
        let factor = Self::TEMPL
            .into_iter()
            .find(|&templ| positions.clone().all(|pos| pos.tick() % templ == 0))
            .unwrap_or(1);
        let plan = match factor {
            4 | 2 => Plan::Repeater,
            3 | 1 => Plan::Piston,
            _ => unreachable!(),
        };
        let song_length = tracks
            .iter()
            .flat_map(|n| n.keys().map(|pos| pos.tick()))
            .max()
            .map_or(0, |t| t + 1);
        let layouts = tracks
            .into_iter()
            .map(|notes| SingleLinearLayout::new(notes, song_length, plan, factor));
        Self(Arranged::new(layouts, Axis::Easting, gap))
    }
}

impl Layout for LinearLayout {
    fn size(&self) -> BlockPos {
        self.0.size()
    }

    fn get_block(&self, pos: BlockPos) -> Option<GenericBlockState> {
        self.0.get_block(pos)
    }
}

// SingleLinearLayout
//
// ++++++++++++============++++++++++++============++++++++++++============

/// A single linear track layout.
pub struct SingleLinearLayout {
    notes: Notes,
    plan: Plan,
    scale: Tick,
    southing: i32,
}

impl SingleLinearLayout {
    pub fn new(notes: Notes, song_length: Tick, plan: Plan, scale: Tick) -> Self {
        let southing = match scale {
            1 => (song_length.div_ceil(2) as i32) * Plan::SOUTHING + Plan::SOUTHING + 1,
            _ => (song_length.div_ceil(scale * 2) as i32) * Plan::SOUTHING + 1,
        };
        Self {
            notes,
            plan,
            scale,
            southing,
        }
    }
}

impl Layout for SingleLinearLayout {
    fn size(&self) -> BlockPos {
        BlockPos::new(self.plan.width(), Plan::ELEVATION, self.southing)
    }

    fn get_block(&self, pos: BlockPos) -> Option<GenericBlockState> {
        let local_pos = BlockPos::new(pos.x, pos.y, self.southing - pos.z - 1);
        Some(self.plan.get_block(&self.notes, local_pos, self.scale))
    }
}

// Plan
//
// ++++++++++++============++++++++++++============++++++++++++============

#[derive(Clone, Copy)]
pub enum Plan {
    Repeater,
    Piston,
}

impl Plan {
    pub const SOUTHING: i32 = 2;

    pub const ELEVATION: i32 = 2;

    pub fn width(&self) -> i32 {
        match self {
            Plan::Repeater => 5,
            Plan::Piston => 6,
        }
    }

    pub fn get_block(&self, notes: &Notes, pos: BlockPos, scale: Tick) -> GenericBlockState {
        let BlockPos {
            x: local_easting,
            y: elevation,
            z: southing,
        } = pos;

        let flat_southing = southing - if local_easting <= 2 { 0 } else { 1 };
        let local_southing = flat_southing.rem_euclid(Self::SOUTHING);

        let note = move |tick, layer| {
            let groups = flat_southing.div_euclid(Self::SOUTHING);
            let head = if scale == 1 { 1 } else { 0 };
            let base_tick = (groups - head) * scale as i32 * 2;
            let tick = (base_tick + tick as i32).try_into().ok()?;
            notes.get(&Position::new(tick, layer))
        };
        let has_branch = match self {
            Plan::Repeater => note(scale, 0).or_else(|| note(scale, 1)).is_some(),
            Plan::Piston => note(3, 0).or_else(|| note(3, 1)).is_some(),
        };
        match (self, has_branch, local_southing, local_easting, elevation) {
            (Plan::Piston, true, 1, 2, 1) => sticky_piston("east"),
            (Plan::Piston, true, 0, 3, 1) => redstone_block(),
            (Plan::Piston, true, 0, 5, 0) => instrument_block(note(3, 0), air),
            (Plan::Piston, true, 0, 5, 1) => note_block(note(3, 0), air),
            (Plan::Piston, true, 1, 4, 0) => instrument_block(note(3, 1), air),
            (Plan::Piston, true, 1, 4, 1) => note_block(note(3, 1), air),
            (Plan::Repeater, true, 1, 2, 0) => chain_block(),
            (Plan::Repeater, true, 1, 2, 1) => repeater((scale / 2).to_string(), "west"),
            (Plan::Repeater, true, 0, 3, 0) => instrument_block(note(scale, 0), chain_block),
            (Plan::Repeater, true, 0, 3, 1) => note_block(note(scale, 0), chain_block),
            (Plan::Repeater, true, 0, 4, 0) => instrument_block(note(scale, 1), air),
            (Plan::Repeater, true, 0, 4, 1) => note_block(note(scale, 1), air),
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
