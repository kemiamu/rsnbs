//! Compact cursor-based layout for NBS song projection.

use crate::schematic::{Layout, chain_block, floor_block, instrument_block};
use crate::schematic::{air, note_block, redstone_wire, repeater};
use crate::{GameTick, Note, RedStoneTick};
use mcdata::{GenericBlockState, util::BlockPos};
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
            southing: columns as i32 * 2,
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
            true => southing + 1,
            false => self.southing - southing,
        };

        if elevation == 0 {
            // is floor
            floor_block()
        } else if southing == 0 {
            // is northern turning
            let row = (local_easting + 1) / 4 * 2;
            let offset = (local_easting + 1) % 4;
            let layout_idx = (elevation + offset % 2 * 3 - 1) as u8;
            let tile = track.get_tile(row, offset as usize / 2);
            tile.map_or_else(air, |t| t.get_block(layout_idx, true))
        } else if southing + 1 == self.southing {
            // is southern turning
            let row = (local_easting + 3) / 4 * 2 - 1;
            let offset = (local_easting + 3) % 4;
            let layout_idx = (elevation + offset % 2 * 3 - 1) as u8;
            let tile = track.get_tile(row, offset as usize / 2);
            tile.map_or_else(air, |t| t.get_block(layout_idx, false))
        } else if is_trunk {
            // is trunk
            let row = local_easting / 2;
            let south_facing = row % 2 == 0;
            let offset = offset(southing, south_facing);
            let layout_idx = elevation as u8 - 1;
            let tile = track.get_tile(row, offset as usize);
            tile.map_or_else(air, |t| t.get_block(layout_idx, south_facing))
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
            tile.map_or_else(air, |t| t.get_block(layout_idx, south_facing))
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

    fn at_row_start(&self) -> bool {
        match self.cols {
            Some(c) => self.len() % c.get() == 0,
            None => self.len() == 0,
        }
    }

    fn at_row_end(&self) -> bool {
        match self.cols {
            Some(c) => (self.len() + 1) % c.get() == 0,
            None => false,
        }
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
            current_tick = redstone_tick;

            while let Some((part, consume)) = Self::_pop_delay(delay, repeater_coarse, &this) {
                this.push(part);
                delay -= consume;
            }

            let stem = match this.at_row_start() {
                true => Tile::TurningDelay(delay),
                false => Tile::Delay(delay),
            };
            let canopy = match (this.at_row_start(), this.at_row_end()) {
                (true, true) => Tile::TurningNode(notes.pop()),
                (true, false) => Tile::TurningTerminal(notes.pop(), notes.pop()),
                (false, true) => Tile::Node(notes.pop(), notes.pop()),
                (false, false) => Tile::Terminal(notes.pop(), notes.pop(), notes.pop()),
            };
            this.push([stem, canopy]);

            if notes.len() > 0 {
                let stem = match this.at_row_start() {
                    true => Tile::TurningLink,
                    false => Tile::Link,
                };
                let canopy = match this.at_row_start() {
                    true => Tile::TurningNode(notes.pop()),
                    false => Tile::Node(notes.pop(), notes.pop()),
                };
                this.push([stem, canopy]);
            }

            loop {
                match (this.at_row_start(), this.at_row_end()) {
                    _ if notes.is_empty() => break,
                    (true, _) if notes.len() <= 2 => {
                        let stem = Tile::TurningLink;
                        let canopy = Tile::TurningTerminal(notes.pop(), notes.pop());
                        this.push([stem, canopy]);
                    }
                    (true, _) => {
                        let stem = Tile::TurningLink;
                        let canopy = Tile::TurningNode(notes.pop());
                        this.push([stem, canopy]);
                    }
                    (false, false) if notes.len() <= 3 => {
                        let stem = Tile::Link;
                        let canopy = Tile::Terminal(notes.pop(), notes.pop(), notes.pop());
                        this.push([stem, canopy]);
                    }
                    (false, _) => {
                        let stem = Tile::Link;
                        let canopy = Tile::Node(notes.pop(), notes.pop());
                        this.push([stem, canopy]);
                    }
                }
            }
        }

        this
    }

    /// Pop a delay tile pair from the start of a gap, if any remains.
    fn _pop_delay(
        delay: RedStoneTick,
        coarse: RedStoneTick,
        track: &Track,
    ) -> Option<([Tile; 2], RedStoneTick)> {
        let chain = track.last().is_some_and(|[_, canopy]| {
            // Chain state if no signal was previously output
            matches!(canopy, Tile::Delay(c) if c == &coarse)
        });
        let at_start = track.at_row_start();
        let at_end = track.at_row_end();
        let pair = |stem_delay: RedStoneTick, canopy: RedStoneTick| {
            let consumed = stem_delay + canopy;
            let stem = Tile::Delay(stem_delay);
            let canopy = match (canopy, at_end) {
                (0, true) => Tile::Node(None, None),
                (0, false) => Tile::Terminal(None, None, None),
                _ => Tile::Delay(canopy),
            };
            Some(([stem, canopy], consumed))
        };
        let turn = |delay: RedStoneTick| {
            let stem = Tile::TurningDelay(delay);
            let canopy = match at_end {
                true => Tile::TurningNode(None),
                false => Tile::TurningTerminal(None, None),
            };
            Some(([stem, canopy], delay))
        };
        match (coarse, chain, at_start, at_end) {
            // Micro-timing (coarse 2..=4)
            // TODO 也许应该尝试用 panic 作为闭包返回
            (2..=4, _, false, false) if delay > coarse * 2 => pair(coarse, coarse),
            (2..=4, true, true, _) => panic!(),
            (2..=4, true, false, true) if delay >= coarse => pair(coarse - 1, 0),
            (2..=4, true, false, false) if delay >= coarse => pair(coarse - 1, 0),
            (2..=4, false, false, false) if delay > coarse => pair(coarse, 0),
            (2..=4, false, true, _) if delay > coarse => turn(coarse),
            (2..=4, false, false, true) if delay > coarse => pair(coarse, 0),
            // Pulse (coarse == 1)
            (1, true, _, _) => panic!(),
            (1, false, true, _) if delay > 1 => turn(coarse),
            (1, false, false, _) if delay > 1 => pair(coarse, 0),
            // Unaffected (else)
            (_, _, true, _) if delay > 4 => turn(4),
            (_, _, false, _) if delay > 8 => pair(4, 4),
            (_, _, false, _) if delay > 4 => pair(4, 0),
            _ => None,
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
    /// Turning delay connection.
    TurningDelay(RedStoneTick),
    /// Turning direct connection.
    TurningLink,
    /// Turning terminal.
    TurningTerminal(Option<Note>, Option<Note>),
    /// Turning node.
    TurningNode(Option<Note>),
}

impl Tile {
    /// Block at the given layout index.
    fn get_block(&self, layout_index: u8, facing_south: bool) -> GenericBlockState {
        // The repeater facing direction is reversed.
        let facing = match facing_south {
            true => "north",
            false => "south",
        };
        match (self, layout_index) {
            (Self::Delay(_), 0) => chain_block(),
            (Self::Delay(delay), 1) => repeater(delay.to_string(), facing),
            (Self::Link, 0) => chain_block(),
            (Self::Link, 1) => redstone_wire(),
            (Self::Terminal(center, _, _), 0) => instrument_block(center, chain_block),
            (Self::Terminal(center, _, _), 1) => note_block(center, chain_block),
            (Self::Terminal(_, left, _), 3) => instrument_block(left, air),
            (Self::Terminal(_, left, _), 4) => note_block(left, air),
            (Self::Terminal(_, _, right), 6) => instrument_block(right, air),
            (Self::Terminal(_, _, right), 7) => note_block(right, air),
            (Self::Node(_, _), 0 | 1) => chain_block(),
            (Self::Node(_, _), 2) => redstone_wire(),
            (Self::Node(left, _), 3) => instrument_block(left, air),
            (Self::Node(left, _), 4) => note_block(left, air),
            (Self::Node(_, right), 6) => instrument_block(right, air),
            (Self::Node(_, right), 7) => note_block(right, air),
            // turning variants – same block placement as regular variants
            (Self::TurningDelay(_), 0 | 3) => chain_block(),
            (Self::TurningDelay(_), 1) => redstone_wire(),
            (Self::TurningDelay(delay), 4) => repeater(delay.to_string(), "west"),
            (Self::TurningLink, 0 | 3) => chain_block(),
            (Self::TurningLink, 1 | 4) => redstone_wire(),
            (Self::TurningTerminal(center, _), 0) => instrument_block(center, chain_block),
            (Self::TurningTerminal(center, _), 1) => note_block(center, chain_block),
            (Self::TurningTerminal(_, side), 3) => instrument_block(side, air),
            (Self::TurningTerminal(_, side), 4) => note_block(side, air),
            (Self::TurningNode(_), 0 | 1) => chain_block(),
            (Self::TurningNode(_), 2) => redstone_wire(),
            (Self::TurningNode(side), 3) => instrument_block(side, air),
            (Self::TurningNode(side), 4) => note_block(side, air),
            _ => air(),
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
