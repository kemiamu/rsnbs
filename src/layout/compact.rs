//! Compact cursor-based layout for NBS song projection.
//!
//! Groups notes into 3×2×3 block groups arranged in a cursor-driven
//! row–column grid. Tracks run side-by-side along easting, groups
//! within a track run along southing, with optional row wrapping.

use crate::schematic::{Layout, chain_block, redstone_wire};
use crate::{GameTick, Note, RedStoneTick, Tick};
use mcdata::{BlockState, GenericBlockState, util::BlockPos};
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::num::{NonZero, NonZeroUsize};
use std::ops::{Deref, DerefMut};

// Track
//
// ++++++++++++============++++++++++++============++++++++++++============

/// A track's groups with its row-column metadata.
struct Track {
    groups: Vec<Group>,
    cols: Option<NonZeroUsize>,
}

impl Track {
    /// Raw constructor. `cols` is `None` when no wrapping is configured.
    fn new(groups: Vec<Group>, cols: Option<NonZeroUsize>) -> Self {
        Self { groups, cols }
    }

    /// Configured columns per row, or `None` for no wrapping (single row).
    fn cols(&self) -> Option<NonZeroUsize> {
        self.cols
    }

    /// Returns `cols()` if set, otherwise falls back to total group count.
    fn cols_or_len(&self) -> usize {
        self.cols.map_or(self.len(), NonZeroUsize::get)
    }

    /// Number of rows occupied by this track.
    fn rows(&self) -> usize {
        self.cols.map_or(1, |c| self.len().div_ceil(c.get()))
    }

    /// Access a group by its logical (row, column) position.
    fn group_at(&self, row: usize, offset: usize) -> Option<&Group> {
        let idx = row * self.cols_or_len() + offset;
        self.groups.get(idx)
    }

    /// `true` if the next push would complete the current row.
    /// Always `false` when no wrapping is configured.
    fn is_row_boundary(&self) -> bool {
        self.cols.is_some_and(|c| (self.len() + 1) % c.get() == 0)
    }

    /// Build a `Track` from timed notes, packing them into 3×2×3 groups.
    fn generate(
        timed_notes: BTreeMap<RedStoneTick, Vec<Note>>,
        coarse: Option<NonZero<GameTick>>,
        cols: Option<NonZeroUsize>,
    ) -> Self {
        let repeater_coarse = coarse.map_or(RedStoneTick::MAX, |l| l.get() / 4);
        let mut this = Self::new(Default::default(), cols);
        let mut current_tick: RedStoneTick = RedStoneTick::MAX;

        for (redstone_tick, mut notes) in timed_notes {
            let mut delay = redstone_tick.wrapping_sub(current_tick);
            let mut remaining = notes.len();

            current_tick = redstone_tick;

            let mut carry = false;
            while let Some((group, consumed)) =
                Self::_pop_delay_group(delay, repeater_coarse, carry, this.is_row_boundary())
            {
                this.push(group);
                delay -= consumed;
                carry = consumed > repeater_coarse;
            }

            this.push(Group::Delayed(delay, notes.pop(), notes.pop(), notes.pop()));
            remaining = remaining.saturating_sub(3);

            if remaining > 0 {
                this.push(Group::Sustain(notes.pop(), notes.pop()));
                remaining = remaining.saturating_sub(2);
            }
            while remaining > 3 {
                this.push(Group::Sustain(notes.pop(), notes.pop()));
                remaining = remaining.saturating_sub(2);
            }
            if remaining > 0 && this.is_row_boundary() {
                this.push(Group::Sustain(notes.pop(), notes.pop()));
                remaining = remaining.saturating_sub(2);
            }
            if remaining > 0 {
                this.push(Group::SustainEnd(notes.pop(), notes.pop(), notes.pop()));
            }
        }

        this
    }

    /// Try to consume part of a delay gap as a `DelayOnly` or `Delayed` group.
    /// Returns `Some((group, ticks_consumed))` or `None` when no more delay groups fit.
    fn _pop_delay_group(
        delay: RedStoneTick,
        coarse: RedStoneTick,
        carry: bool,
        wrap: bool,
    ) -> Option<(Group, RedStoneTick)> {
        if (2..=4).contains(&coarse) {
            if delay > coarse * 2 && !wrap {
                let delay = coarse * 2;
                Some((Group::DelayOnly(coarse, coarse), delay))
            } else if delay > coarse * 2 {
                let delay = coarse * 2 - 1;
                Some((Group::DelayOnly(coarse, coarse - 1), delay))
            } else if delay == coarse * 2 && carry {
                let delay = coarse + 1;
                Some((Group::DelayOnly(coarse, 1), delay))
            } else if delay >= coarse && carry {
                let delay = coarse - 1;
                Some((Group::Delayed(delay, None, None, None), delay))
            } else if delay > coarse && !carry {
                Some((Group::Delayed(coarse, None, None, None), coarse))
            } else {
                None
            }
        } else if coarse == 1 {
            if delay > coarse {
                Some((Group::Delayed(coarse, None, None, None), coarse))
            } else {
                None
            }
        } else {
            if delay > 8 {
                Some((Group::DelayOnly(4, 4), 8))
            } else if delay > 4 {
                Some((Group::Delayed(4, None, None, None), 4))
            } else {
                None
            }
        }
    }
}

impl Deref for Track {
    type Target = Vec<Group>;

    fn deref(&self) -> &Vec<Group> {
        &self.groups
    }
}

impl DerefMut for Track {
    fn deref_mut(&mut self) -> &mut Vec<Group> {
        &mut self.groups
    }
}

// CompactLayout
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Compact noteblocks layout
pub struct CompactLayout {
    /// `(groups, start_easting, extent_easting)` for each track.
    tracks: Vec<(Track, i32, i32)>,
    /// Easting extent (X dimension) of the layout.
    easting: i32,
    /// Southing extent (Z dimension) of the layout.
    southing: i32,
}

impl CompactLayout {
    /// Total elevation (Y) of the layout, in blocks.
    const ELEVATION: i32 = 4;

    /// Create a compact layout from note tracks.
    pub fn new<N>(
        tracks: impl IntoIterator<Item = (N, Option<NonZero<GameTick>>)>,
        wrap_length: Option<NonZeroUsize>,
        gap: u32,
    ) -> Self
    where
        N: IntoIterator<Item = (GameTick, Vec<Note>)>,
    {
        let mut cursor: i32 = -(gap as i32);
        let place_track = |track: Track| {
            let start = cursor + gap as i32;
            let extent = (track.rows() as i32) * 2 + 1;
            cursor = start + extent;
            (track, start, extent)
        };
        let tracks: Vec<(Track, i32, i32)> = tracks
            .into_iter()
            .flat_map(|(notes, coarse)| split_even_odd(notes, coarse))
            .filter(|(notes, _)| !notes.is_empty())
            .map(|(notes_map, coarse)| Track::generate(notes_map, coarse, wrap_length))
            .map(place_track)
            .collect();

        let columns = tracks
            .iter()
            .map(|(track, _, _)| track.cols_or_len())
            .max()
            .unwrap_or(0);

        Self {
            easting: cursor,
            southing: columns as i32 * 2 + 2,
            tracks,
        }
    }
}

impl Layout for CompactLayout {
    fn size(&self) -> BlockPos {
        BlockPos::new(self.easting, Self::ELEVATION, self.southing)
    }

    fn get_block(&self, pos: BlockPos) -> GenericBlockState {
        let (easting, elevation, southing) = (pos.x, pos.y, pos.z);

        debug_assert!((0..Self::ELEVATION).contains(&elevation), "y out of range");
        debug_assert!((0..self.easting).contains(&easting), "x out of range");
        debug_assert!((0..self.southing).contains(&southing), "z out of range");

        // find track
        debug_assert!(self.tracks.iter().any(|(_, start, _)| *start == 0));
        let track_idx = self.tracks.partition_point(|(_, s, _)| *s <= easting) - 1;
        let (track, track_start, track_extent) = &self.tracks[track_idx];
        let local_easting = easting - track_start;

        let is_trunk = local_easting % 2 == 1;
        let advance = |southing: usize, south_facing: bool| match south_facing {
            true => southing - 1,
            false => (self.southing as usize - 3) - (southing - 1),
        };

        // is floor
        if elevation == 0 {
            return floor_block();
        }

        // is turning
        if southing == 0 || southing + 1 == self.southing {
            if (southing == 0 && ((local_easting as usize).saturating_sub(2)) % 4 != 0)
                || (southing + 1 == self.southing
                    && local_easting % 4 != 0
                    && (track.cols().map_or(false, |c| c.get() < track.len())
                        || local_easting >= 3)
                    && (track.len() / track.cols_or_len() % 2 == 0
                        || local_easting + 2 < *track_extent))
            {
                if elevation == 1 {
                    return chain_block();
                } else if elevation == 2 {
                    return redstone_wire();
                }
            }
            return GenericBlockState::air();
        }

        // is group
        let (group, layout_idx, south_facing) = if is_trunk {
            let row: usize = local_easting as usize / 2;
            let south_facing = row % 2 == 0;
            let offset = advance(southing as usize, south_facing);
            let Some(group) = track.group_at(row, offset / 2) else {
                return GenericBlockState::air();
            };
            let layout_idx = (offset % 2 * 3 + elevation as usize - 1) as u8;
            (group, layout_idx, south_facing)
        } else if local_easting == 0 {
            let row: usize = local_easting as usize / 2;
            let south_facing = row % 2 == 0;
            let offset = advance(southing as usize, south_facing);
            if offset % 2 == 0 {
                return GenericBlockState::air();
            }
            let Some(group) = track.group_at(row, offset / 2) else {
                return GenericBlockState::air();
            };
            let layout_idx = (if south_facing { 9 } else { 6 } + elevation as usize - 1) as u8;
            (group, layout_idx, south_facing)
        } else if local_easting + 1 == *track_extent {
            let row: usize = local_easting as usize / 2 - 1;
            let south_facing = row % 2 == 0;
            let offset = advance(southing as usize, south_facing);
            if offset % 2 == 0 {
                return GenericBlockState::air();
            }
            let Some(group) = track.group_at(row, offset / 2) else {
                return GenericBlockState::air();
            };
            let layout_idx = (if south_facing { 6 } else { 9 } + elevation as usize - 1) as u8;
            (group, layout_idx, south_facing)
        } else {
            let row: usize = local_easting as usize / 2
                - match southing % 2 == 0 {
                    true => 1,
                    false => 0,
                };
            let south_facing = row % 2 == 0;
            let offset = advance(southing as usize, south_facing);
            let Some(group) = track.group_at(row, offset / 2) else {
                return GenericBlockState::air();
            };
            let layout_idx = (if south_facing { 6 } else { 9 } + elevation as usize - 1) as u8;
            (group, layout_idx, south_facing)
        };

        group.get_block(&layout_idx, south_facing)
    }
}

// Group
//
// ++++++++++++============++++++++++++============++++++++++++============

#[derive(Debug, Clone)]
enum Group {
    DelayOnly(RedStoneTick, RedStoneTick),
    Delayed(RedStoneTick, Option<Note>, Option<Note>, Option<Note>),
    Sustain(Option<Note>, Option<Note>),
    SustainEnd(Option<Note>, Option<Note>, Option<Note>),
}

impl Group {
    fn get_block(&self, index: &u8, facing_south: bool) -> GenericBlockState {
        // The repeater facing direction is reversed.
        let reversed_facing: Cow<'static, str> = match facing_south {
            true => "north".into(),
            false => "south".into(),
        };
        let repeater = |delay: &RedStoneTick| GenericBlockState {
            name: "minecraft:repeater".into(),
            properties: HashMap::from([
                ("delay".into(), delay.to_string().into()),
                ("facing".into(), reversed_facing.clone()),
                ("locked".into(), "false".into()),
                ("powered".into(), "false".into()),
            ]),
        };
        let note_block = |note: &Option<Note>, fallback: fn() -> GenericBlockState| {
            note.as_ref()
                .and_then(|n| n.note_block_state())
                .unwrap_or_else(fallback)
        };
        let instrument_block = |note: &Option<Note>, fallback: fn() -> GenericBlockState| {
            note.as_ref()
                .and_then(|n| n.instrument.instrument_block())
                .unwrap_or_else(fallback)
        };

        match self {
            Group::DelayOnly(first, second) => match index {
                0 | 3 => chain_block(),
                1 => repeater(first),
                4 => repeater(second),
                _ => GenericBlockState::air(),
            },
            Group::Delayed(delay, center, left, right) => match index {
                0 => chain_block(),
                1 => repeater(delay),
                3 => instrument_block(center, chain_block),
                4 => note_block(center, chain_block),
                6 => instrument_block(left, GenericBlockState::air),
                7 => note_block(left, GenericBlockState::air),
                9 => instrument_block(right, GenericBlockState::air),
                10 => note_block(right, GenericBlockState::air),
                _ => GenericBlockState::air(),
            },
            Group::Sustain(left, right) => match index {
                0 | 3 | 4 => chain_block(),
                1 | 5 => redstone_wire(),
                6 => instrument_block(left, GenericBlockState::air),
                7 => note_block(left, GenericBlockState::air),
                9 => instrument_block(right, GenericBlockState::air),
                10 => note_block(right, GenericBlockState::air),
                _ => GenericBlockState::air(),
            },
            Group::SustainEnd(center, left, right) => match index {
                0 => chain_block(),
                1 => redstone_wire(),
                3 => instrument_block(center, chain_block),
                4 => note_block(center, chain_block),
                6 => instrument_block(left, GenericBlockState::air),
                7 => note_block(left, GenericBlockState::air),
                9 => instrument_block(right, GenericBlockState::air),
                10 => note_block(right, GenericBlockState::air),
                _ => GenericBlockState::air(),
            },
        }
    }
}

// Helpers
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Split game tick notes into even/odd redstone tick buckets.
fn split_even_odd<N>(
    tracks: N,
    coarse: Option<NonZero<GameTick>>,
) -> impl Iterator<Item = (BTreeMap<RedStoneTick, Vec<Note>>, Option<NonZero<GameTick>>)>
where
    N: IntoIterator<Item = (GameTick, Vec<Note>)>,
{
    let mut buckets: [BTreeMap<RedStoneTick, Vec<Note>>; 2] = Default::default();
    for (game_tick, notes) in tracks {
        buckets[(game_tick & 1) as usize]
            .entry(game_tick / 2)
            .or_default()
            .extend(notes);
    }
    buckets.into_iter().map(move |m| (m, coarse))
}

fn floor_block() -> GenericBlockState {
    GenericBlockState {
        name: "minecraft:white_concrete".into(),
        properties: Default::default(),
    }
}
