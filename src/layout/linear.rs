//! Linear time-proportional layout for NBS song projection.

use crate::schematic::{Layout, air, chain_block, instrument_block, note_block};
use crate::schematic::{redstone_block, repeater, sticky_piston};
use crate::{Index, Notes, Position, Tick};
use mcdata::{GenericBlockState, util::BlockPos};

// LinearLayout
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Linear noteblocks layout
pub struct LinearLayout {
    track_notes: Vec<Notes>,
    plan: Plan,
    scale: Tick,
    gap: i32,
    easting: i32,
    southing: i32,
}

impl LinearLayout {
    const TEMPL: [Tick; 3] = [4, 2, 3];

    /// Create a linear layout from per-tick notes.
    pub fn new(notes: Notes, gap: u32) -> Self {
        let (scale, plan) = Self::_scale(&notes);
        let track_notes = Self::_group_tracks(notes);
        let easting = Self::_width(&track_notes, plan, gap);
        let southing = Self::_length(&track_notes, scale);

        Self {
            track_notes,
            plan,
            scale,
            gap: gap as i32,
            easting,
            southing,
        }
    }

    fn _scale(notes: &Notes) -> (Tick, Plan) {
        let factor = Self::TEMPL
            .into_iter()
            .find(|templ| notes.keys().all(|pos| pos.tick() % templ == 0))
            .unwrap_or(1);
        let plan = match factor {
            4 | 2 => Plan::Repeater,
            _ => Plan::Piston,
        };
        (factor, plan)
    }

    /// Group consecutive layers, remapping each track's notes to 0-based layer indices.
    fn _group_tracks(notes: Notes) -> Vec<Notes> {
        let max = notes.keys().map(|p| p.layer()).max().unwrap_or_default();
        let mut cuts: Vec<Index> = vec![0];
        for x in (0..=max).filter(|x| !notes.keys().any(|p| p.layer() == *x)) {
            cuts.push(x + 1);
        }
        let mut buckets: Vec<Notes> = vec![Default::default(); cuts.len()];
        for (pos, note) in notes {
            let track = cuts.partition_point(|&c| c <= pos.layer()) - 1;
            buckets[track].insert(Position(pos.tick(), pos.layer() - cuts[track]), note);
        }
        buckets
    }

    fn _width(track_notes: &[Notes], plan: Plan, gap: u32) -> i32 {
        (plan.width() + gap as i32) * track_notes.len() as i32 - gap as i32
    }

    fn _length(track_notes: &[Notes], scale: Tick) -> i32 {
        let ticks = track_notes
            .iter()
            .flat_map(|n| n.keys().map(|pos| pos.tick()))
            .max()
            .map(|t| t + 1)
            .unwrap_or_default();
        match scale {
            1 => ticks.div_ceil(scale * 2) as i32 * Plan::SOUTHING + 1 + Plan::SOUTHING,
            _ => ticks.div_ceil(scale * 2) as i32 * Plan::SOUTHING + 1,
        }
    }
}

impl Layout for LinearLayout {
    fn size(&self) -> BlockPos {
        BlockPos::new(self.easting, Plan::ELEVATION, self.southing)
    }

    fn get_block(&self, pos: BlockPos) -> GenericBlockState {
        let track_idx = (pos.x / (self.plan.width() + self.gap)) as usize;
        let local_pos = BlockPos::new(
            pos.x % (self.plan.width() + self.gap),
            pos.y,
            self.southing - pos.z - 1,
        );
        match self.track_notes.get(track_idx) {
            Some(track_notes) => self.plan.get_block(track_notes, local_pos, self.scale),
            None => air(),
        }
    }
}

// Plan
//
// ++++++++++++============++++++++++++============++++++++++++============

#[derive(Clone, Copy)]
enum Plan {
    Repeater,
    Piston,
}

impl Plan {
    const SOUTHING: i32 = 2;

    const ELEVATION: i32 = 2;

    fn width(&self) -> i32 {
        match self {
            Plan::Repeater => 5,
            Plan::Piston => 6,
        }
    }

    fn get_block(&self, notes: &Notes, pos: BlockPos, scale: Tick) -> GenericBlockState {
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
            notes.get(&Position(tick, layer))
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
