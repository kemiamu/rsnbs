//! Compact cursor-based layout for NBS song projection.

use crate::schematic::{Layout, chain_block, floor_block, instrument_block};
use crate::schematic::{note_block, redstone_wire, repeater};
use crate::{GameTick, Note, RedStoneTick};
use mcdata::{BlockState, GenericBlockState, util::BlockPos};
use std::collections::BTreeMap;
use std::num::{NonZero, NonZeroUsize};
use std::ops::{Deref, DerefMut};

// CompactLayout
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Compact noteblocks layout
pub struct CompactLayout {
    /// `(track, start_easting)` for each track.
    tracks: Vec<(Track, i32)>,
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
            cursor = start + (track.rows() as i32) * 2 + 1;
            (track, start)
        };
        let tracks: Vec<(Track, i32)> = tracks
            .into_iter()
            .flat_map(|(notes, coarse)| split_even_odd(notes, coarse))
            .filter(|(notes, _)| !notes.is_empty())
            .map(|(notes_map, coarse)| Track::new(notes_map, coarse, wrap_length))
            .map(place_track)
            .collect();

        let columns = tracks
            .iter()
            .map(|(track, _)| track.cols_or_len())
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
        let BlockPos {
            x: easting,
            y: elevation,
            z: southing,
        } = pos;

        debug_assert!((0..Self::ELEVATION).contains(&elevation), "y out of range");
        debug_assert!((0..self.easting).contains(&easting), "x out of range");
        debug_assert!((0..self.southing).contains(&southing), "z out of range");

        // find track
        debug_assert!(self.tracks.iter().any(|(_, start)| *start == 0));
        let track_idx = self.tracks.partition_point(|(_, s)| *s <= easting) - 1;
        let (track, track_start) = &self.tracks[track_idx];
        let local_easting = easting - track_start;

        let is_trunk = local_easting % 2 == 1;
        let offset = |southing, south_facing| match south_facing {
            true => southing - 1,
            false => self.southing - southing - 2,
        };
        let turning_block = |row, offset| match offset != 3 && track.rows() as i32 > row {
            true if elevation == 1 => chain_block(),
            true if elevation == 2 => redstone_wire(),
            _ => GenericBlockState::air(),
        };

        if elevation == 0 {
            // is floor
            floor_block()
        } else if southing == 0 {
            // is northern turning
            let row = (local_easting + 1) / 4 * 2;
            let offset = (local_easting + 1) % 4;
            turning_block(row, offset)
        } else if southing + 1 == self.southing {
            // is southern turning
            let row = (local_easting + 3) / 4 * 2 - 1;
            let offset = (local_easting + 3) % 4;
            turning_block(row, offset)
        } else if is_trunk {
            // is trunk
            let row = local_easting / 2;
            let south_facing = row % 2 == 0;
            let offset = offset(southing, south_facing);
            let layout_idx = elevation as u8 - 1;
            let tile = track.get_tile(row, offset as usize);
            tile.map_or_else(GenericBlockState::air, |t| {
                t.get_block(layout_idx, south_facing)
            })
        } else {
            // is cogs
            let south_facing = southing % 2 == 0;
            let row = match south_facing {
                true => local_easting / 4 * 2,
                false => (local_easting + 2) / 4 * 2 - 1,
            };
            let offset = offset(southing, south_facing);
            let layout_idx = match local_easting / 2 % 2 == 0 {
                true => elevation + 2,
                false => elevation + 5,
            } as u8;
            let tile = track.get_tile(row, offset as usize);
            tile.map_or_else(GenericBlockState::air, |t| {
                t.get_block(layout_idx, south_facing)
            })
        }
    }
}

// Track
//
// ++++++++++++============++++++++++++============++++++++++++============

/// A track's tiles with its row-column metadata.
struct Track {
    tiles: Vec<[Tile; 2]>,
    cols: Option<NonZeroUsize>,
}

impl Track {
    /// Returns `cols()` if set, otherwise falls back to total tile count.
    fn cols_or_len(&self) -> usize {
        self.cols.map_or(self.len(), NonZeroUsize::get)
    }

    /// Number of rows occupied by this track.
    fn rows(&self) -> usize {
        self.cols.map_or(1, |c| self.len().div_ceil(c.get()))
    }

    /// Get tile at (row, offset).
    fn get_tile(&self, row: impl TryInto<usize>, offset: usize) -> Option<&Tile> {
        let idx = row.try_into().ok()? * self.cols_or_len() + offset / 2;
        let [stem, canopy] = self.tiles.get(idx)?;
        match offset % 2 == 0 {
            true => Some(stem),
            false => Some(canopy),
        }
    }

    /// `true` if the next push would complete the current row.
    fn is_row_boundary(&self) -> bool {
        self.cols.is_some_and(|c| (self.len() + 1) % c.get() == 0)
    }

    /// Build a `Track` from timed notes, packing them into tiles.
    fn new(
        timed_notes: BTreeMap<RedStoneTick, Vec<Note>>,
        coarse: Option<NonZero<GameTick>>,
        columns: Option<NonZeroUsize>,
    ) -> Self {
        let repeater_coarse = coarse.map_or(RedStoneTick::MAX, |l| l.get() / 4);
        let mut this = Self {
            tiles: Default::default(),
            cols: columns,
        };
        let mut current_tick: RedStoneTick = RedStoneTick::MAX;

        for (redstone_tick, mut notes) in timed_notes {
            let mut delay = redstone_tick.wrapping_sub(current_tick);
            let mut repeater_chain = false;
            current_tick = redstone_tick;

            while let Some((part, chain, consume)) = Self::_pop_delay_group(
                delay,
                repeater_coarse,
                repeater_chain,
                this.is_row_boundary(),
            ) {
                this.push(part);
                repeater_chain = chain;
                delay -= consume;
            }

            if notes.len() > 0 {
                let stem = Tile::Delay(delay);
                let canopy = Tile::Terminal(notes.pop(), notes.pop(), notes.pop());
                this.push([stem, canopy]);
            }
            if notes.len() > 0 {
                let stem = Tile::Link;
                let canopy = Tile::Node(notes.pop(), notes.pop());
                this.push([stem, canopy]);
            }
            while notes.len() > 3 || (notes.len() > 0 && this.is_row_boundary()) {
                let stem = Tile::Link;
                let canopy = Tile::Node(notes.pop(), notes.pop());
                this.push([stem, canopy]);
            }
            if notes.len() > 0 {
                let stem = Tile::Link;
                let canopy = Tile::Terminal(notes.pop(), notes.pop(), notes.pop());
                this.push([stem, canopy]);
            }
        }

        this
    }

    /// Pop a delay tile pair from the start of a gap, if any remains.
    fn _pop_delay_group(
        delay: RedStoneTick,
        coarse: RedStoneTick,
        chain: bool,
        wrap: bool,
    ) -> Option<([Tile; 2], bool, RedStoneTick)> {
        if (2..=4).contains(&coarse) {
            if delay > coarse * 2 && !wrap {
                let stem = Tile::Delay(coarse);
                let canopy = Tile::Delay(coarse);
                Some(([stem, canopy], true, coarse * 2))
            } else if delay > coarse * 2 {
                let stem = Tile::Delay(coarse);
                let canopy = Tile::Delay(coarse - 1);
                Some(([stem, canopy], false, coarse * 2 - 1))
            } else if delay == coarse * 2 && chain {
                let stem = Tile::Delay(coarse);
                let canopy = Tile::Delay(1);
                Some(([stem, canopy], false, coarse + 1))
            } else if delay >= coarse && chain {
                let stem = Tile::Delay(coarse - 1);
                let canopy = Tile::Terminal(None, None, None);
                Some(([stem, canopy], false, coarse - 1))
            } else if delay > coarse && !chain {
                let stem = Tile::Delay(coarse);
                let canopy = Tile::Terminal(None, None, None);
                Some(([stem, canopy], false, coarse))
            } else {
                None
            }
        } else if coarse == 1 {
            if delay > coarse {
                let stem = Tile::Delay(coarse);
                let canopy = Tile::Terminal(None, None, None);
                Some(([stem, canopy], false, coarse))
            } else {
                None
            }
        } else {
            if delay > 8 {
                let stem = Tile::Delay(coarse);
                let canopy = Tile::Delay(coarse);
                Some(([stem, canopy], false, coarse * 2))
            } else if delay > 4 {
                let stem = Tile::Delay(coarse);
                let canopy = Tile::Terminal(None, None, None);
                Some(([stem, canopy], false, coarse))
            } else {
                None
            }
        }
    }
}

impl Deref for Track {
    fn deref(&self) -> &Vec<[Tile; 2]> {
        &self.tiles
    }
    type Target = Vec<[Tile; 2]>;
}

impl DerefMut for Track {
    fn deref_mut(&mut self) -> &mut Vec<[Tile; 2]> {
        &mut self.tiles
    }
}

// Tile
//
// ++++++++++++============++++++++++++============++++++++++++============

/// A stem–canopy tile pair.
enum Tile {
    /// Delay connection.
    Delay(RedStoneTick),
    /// Direct connection.
    Link,
    /// Terminal.
    Terminal(Option<Note>, Option<Note>, Option<Note>),
    /// Node.
    Node(Option<Note>, Option<Note>),
}

impl Tile {
    /// Block at the given layout index.
    fn get_block(&self, layout_index: u8, facing_south: bool) -> GenericBlockState {
        // The repeater facing direction is reversed.
        let facing = match facing_south {
            true => "north",
            false => "south",
        };
        match self {
            Self::Delay(delay) => match layout_index {
                0 => chain_block(),
                1 => repeater(delay.to_string(), facing),
                _ => GenericBlockState::air(),
            },
            Self::Link => match layout_index {
                0 => chain_block(),
                1 => redstone_wire(),
                _ => GenericBlockState::air(),
            },
            Self::Terminal(center, left, right) => match layout_index {
                0 => instrument_block(center.as_ref(), chain_block),
                1 => note_block(center.as_ref(), chain_block),
                3 => instrument_block(left.as_ref(), GenericBlockState::air),
                4 => note_block(left.as_ref(), GenericBlockState::air),
                6 => instrument_block(right.as_ref(), GenericBlockState::air),
                7 => note_block(right.as_ref(), GenericBlockState::air),
                _ => GenericBlockState::air(),
            },
            Self::Node(left, right) => match layout_index {
                0 => chain_block(),
                1 => chain_block(),
                2 => redstone_wire(),
                3 => instrument_block(left.as_ref(), GenericBlockState::air),
                4 => note_block(left.as_ref(), GenericBlockState::air),
                6 => instrument_block(right.as_ref(), GenericBlockState::air),
                7 => note_block(right.as_ref(), GenericBlockState::air),
                _ => GenericBlockState::air(),
            },
        }
    }
}

// Helpers
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Split game tick notes into even/odd redstone tick buckets.
fn split_even_odd(
    tracks: impl IntoIterator<Item = (GameTick, Vec<Note>)>,
    coarse: Option<NonZero<GameTick>>,
) -> impl Iterator<Item = (BTreeMap<RedStoneTick, Vec<Note>>, Option<NonZero<GameTick>>)> {
    let mut buckets: [BTreeMap<RedStoneTick, Vec<Note>>; 2] = Default::default();
    for (game_tick, notes) in tracks {
        buckets[(game_tick & 1) as usize]
            .entry(game_tick / 2)
            .or_default()
            .extend(notes);
    }
    buckets.into_iter().map(move |m| (m, coarse))
}
