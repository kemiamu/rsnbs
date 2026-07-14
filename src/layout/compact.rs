//! Compact note block layouts for NBS song projection.

use crate::schematic::{Layout, chain_block, instrument_block};
use crate::schematic::{air, note_block, redstone_wire, repeater};
use crate::{GameTick, Note, RedStoneTick};
use mcdata::{GenericBlockState, util::BlockPos};
use std::collections::BTreeMap;
use std::iter;
use std::num::{NonZero, NonZeroUsize};
use std::ops::{Deref, DerefMut};

// MultiCompactLayout
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Multiple compact note block tracks placed side-by-side.
///
/// Each song track is split into even/odd redstone tick sub-tracks,
/// each built as a [`CompactLayout`], then arranged east-to-west
/// with configurable spacing between song tracks.
pub struct MultiCompactLayout {
    bands: Vec<(CompactLayout, i32)>,
    easting: i32,
    southing: i32,
}

impl MultiCompactLayout {
    /// Create a multi-track compact layout from multiple note groups.
    pub fn new<N>(
        tracks: impl IntoIterator<Item = (N, Option<NonZero<GameTick>>)>,
        wrap_length: Option<NonZeroUsize>,
        gap: u32,
    ) -> Self
    where
        N: IntoIterator<Item = (GameTick, Vec<Note>)>,
    {
        let mut cursor: i32 = -(gap as i32);
        let place_band = |band: CompactLayout| {
            let start = cursor + gap as i32;
            cursor = start + band.easting;
            (band, start)
        };
        let bands: Vec<(CompactLayout, i32)> = tracks
            .into_iter()
            .flat_map(|(notes, coarse)| split_even_odd(notes, coarse))
            .filter(|(notes, _)| !notes.is_empty())
            .map(|(notes, coarse)| CompactLayout::new(notes, coarse, wrap_length))
            .map(place_band)
            .collect();
        let southing = bands
            .iter()
            .map(|(band, _)| band.southing)
            .max()
            .unwrap_or(0);
        Self {
            bands,
            easting: cursor,
            southing,
        }
    }
}

impl Layout for MultiCompactLayout {
    fn size(&self) -> BlockPos {
        BlockPos::new(self.easting, CompactLayout::ELEVATION, self.southing)
    }

    fn get_block(&self, pos: BlockPos) -> GenericBlockState {
        debug_assert!((0..self.southing).contains(&pos.z), "z out of range");
        let idx = self.bands.partition_point(|(_, s)| *s <= pos.x) - 1;
        let (band, start) = &self.bands[idx];
        let local_easting = pos.x - start;
        match local_easting < band.easting && pos.z < band.southing {
            true => band.get_block(BlockPos::new(local_easting, pos.y, pos.z)),
            false => air(),
        }
    }
}

// CompactLayout
//
// ++++++++++++============++++++++++++============++++++++++++============

/// A single compact note block track.
///
/// One redstone sub-track's tiles arranged in a compact 3-high
/// zigzag pattern with tooth-interlocked rows.
pub struct CompactLayout {
    track: Track,
    easting: i32,
    southing: i32,
}

impl CompactLayout {
    const ELEVATION: i32 = 3;

    /// Create a compact layout from redstone-tick–grouped notes.
    ///
    /// The input must already be split into a single redstone tick line.
    /// See [`MultiCompactLayout`] for the high-level constructor that handles
    /// the split automatically.
    pub fn new(
        notes: BTreeMap<RedStoneTick, Vec<Note>>,
        coarse: Option<NonZero<GameTick>>,
        wrap_length: Option<NonZeroUsize>,
    ) -> Self {
        let track = Track::new(notes, coarse, wrap_length);
        let easting = (track.rows() as i32) * 2 + 1;
        let southing = track.cols_or_len() as i32;
        Self {
            track,
            easting,
            southing,
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

        let tile_col = |s: i32, row: i32| match row & 1 {
            0 => s + 1,
            _ => self.southing - s,
        };

        if southing == 0 {
            // North edge turn
            let easting = easting + 1;
            let group = easting & 3;
            let row = easting / 4 * 2;
            let col = group as usize / 2;
            let layout_idx = (elevation + (group & 1) * 3) as u8;
            self.track.block_at(row, col, layout_idx)
        } else if southing + 1 == self.southing {
            // South edge turn
            let easting = easting + 3;
            let group = easting & 3;
            let row = easting / 4 * 2 - 1;
            let col = group as usize / 2;
            let layout_idx = (elevation + (group & 1) * 3) as u8;
            self.track.block_at(row, col, layout_idx)
        } else if easting & 1 == 1 {
            // Trunk row
            let row = easting / 2;
            let col = tile_col(southing, row) as usize;
            self.track.block_at(row, col, elevation as u8)
        } else {
            // Tooth row
            let cell = easting / 2;
            let zig = (cell + southing) & 1;
            let row = cell - zig;
            let col = tile_col(southing, row) as usize;
            let layout_idx = (elevation + 3 + zig * 3) as u8;
            self.track.block_at(row, col, layout_idx)
        }
    }
}

// Track
//
// ++++++++++++============++++++++++++============++++++++++++============

/// A track's tiles with its row-column metadata.
struct Track {
    tiles: Vec<Tile>,
    cols: Option<NonZeroUsize>,
}

impl Track {
    fn rows(&self) -> usize {
        self.cols.map_or(1, |c| self.len().div_ceil(c.get()))
    }

    fn cols_or_len(&self) -> usize {
        self.cols.map_or(self.len(), |c| c.get())
    }

    fn get_tile(&self, row: impl TryInto<usize>, offset: usize) -> Option<&Tile> {
        self.tiles
            .get(row.try_into().ok()? * self.cols_or_len() + offset)
    }

    fn block_at(&self, row: i32, col: usize, layout_idx: u8) -> GenericBlockState {
        let repeater_facing = match ((row & 1) == 0, col < 2) {
            (_, true) => "west",
            (true, false) => "north",
            (false, false) => "south",
        };
        self.get_tile(row, col)
            .map_or_else(air, |t| t.get_block(layout_idx, repeater_facing))
    }

    fn at_row_start(&self) -> bool {
        match self.cols {
            Some(c) => self.len() % c.get() == 0,
            None => true,
        }
    }

    fn at_row_end(&self) -> bool {
        match self.cols {
            Some(c) => (self.len() + 2) % c.get() == 0,
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
        let mut track = Self {
            tiles: Default::default(),
            cols: columns.map(|c| NonZeroUsize::new(c.get() * 2).unwrap()),
        };
        let mut current_tick: RedStoneTick = RedStoneTick::MAX;

        for (redstone_tick, mut notes) in timed_notes {
            let mut delay = redstone_tick.wrapping_sub(current_tick);
            current_tick = redstone_tick;

            while let Some((stem, canopy, consume)) =
                Self::_pop_delay(delay, repeater_coarse, &track)
            {
                track.push(stem);
                track.push(canopy);
                delay -= consume;
            }

            let at_start = track.at_row_start();
            let at_end = track.at_row_end();
            let stem = Tile::stem(delay, at_start);
            let canopy = Tile::canopy(iter::from_fn(|| notes.pop()), at_start, !at_end);
            track.push(stem);
            track.push(canopy);

            if !notes.is_empty() {
                let at_start = track.at_row_start();
                let is_terminal = at_start && notes.len() <= 2;
                let stem = Tile::stem(0, at_start);
                let canopy = Tile::canopy(iter::from_fn(|| notes.pop()), at_start, is_terminal);
                track.push(stem);
                track.push(canopy);
            }
            while !notes.is_empty() {
                let at_start = track.at_row_start();
                let at_end = track.at_row_end();
                let is_terminal = !at_end && notes.len() <= if at_start { 2 } else { 3 };
                let stem = Tile::stem(0, at_start);
                let canopy = Tile::canopy(iter::from_fn(|| notes.pop()), at_start, is_terminal);
                track.push(stem);
                track.push(canopy);
            }
        }
        track
    }

    fn _pop_delay(
        delay: RedStoneTick,
        coarse: RedStoneTick,
        track: &Track,
    ) -> Option<(Tile, Tile, RedStoneTick)> {
        let chain = track.last().is_some_and(|canopy| {
            // Chain state if no signal was previously output
            matches!(canopy, Tile::Delay(c) if c == &coarse)
        });
        let at_start = track.at_row_start();
        let at_end = track.at_row_end();
        let pair = |stem, canopy| Self::_place_delay(stem, canopy, at_start);
        let turn = |stem| Self::_place_delay(stem, 0, at_start);

        debug_assert!(!((2..=4).contains(&coarse) && chain && at_start));
        debug_assert!(!(coarse == 1 && chain));

        match (coarse, chain, at_start, at_end) {
            // Micro-timing (coarse 2..=4)
            (2..=4, _, false, false) if delay > coarse * 2 => pair(coarse, coarse),
            (2..=4, true, false, _) if delay >= coarse => pair(coarse - 1, 0),
            (2..=4, false, false, _) if delay > coarse => pair(coarse, 0),
            (2..=4, false, true, _) if delay > coarse => turn(coarse),
            // Pulse (coarse == 1)
            (1, false, true, _) if delay > 1 => turn(coarse),
            (1, false, false, _) if delay > 1 => pair(coarse, 0),
            // Unaffected (else)
            (_, _, true, _) if delay > 4 => turn(4),
            (_, _, false, _) if delay > 8 => pair(4, 4),
            (_, _, false, _) if delay > 4 => pair(4, 0),
            _ => None,
        }
    }

    fn _place_delay(
        stem_delay: RedStoneTick,
        canopy_delay: RedStoneTick,
        at_start: bool,
    ) -> Option<(Tile, Tile, RedStoneTick)> {
        debug_assert!(!(canopy_delay != 0 && at_start));

        let stem = Tile::stem(stem_delay, at_start);
        let canopy = match canopy_delay {
            0 => Tile::canopy(iter::empty(), at_start, true),
            _ => Tile::stem(canopy_delay, false),
        };
        Some((stem, canopy, stem_delay + canopy_delay))
    }
}

impl Deref for Track {
    fn deref(&self) -> &Vec<Tile> {
        &self.tiles
    }
    type Target = Vec<Tile>;
}

impl DerefMut for Track {
    fn deref_mut(&mut self) -> &mut Vec<Tile> {
        &mut self.tiles
    }
}

// Tile
//
// ++++++++++++============++++++++++++============++++++++++++============

/// A stem–canopy tile pair.
enum Tile {
    Delay(RedStoneTick),
    Link,
    Terminal(Option<Note>, Option<Note>, Option<Note>),
    Node(Option<Note>, Option<Note>),
    TurningDelay(RedStoneTick),
    TurningLink,
    TurningTerminal(Option<Note>, Option<Note>),
    TurningNode(Option<Note>),
}

impl Tile {
    fn stem(delay: RedStoneTick, is_turning: bool) -> Tile {
        match (delay, is_turning) {
            (0, true) => Tile::TurningLink,
            (0, false) => Tile::Link,
            (_, true) => Tile::TurningDelay(delay),
            (_, false) => Tile::Delay(delay),
        }
    }

    fn canopy(mut notes: impl Iterator<Item = Note>, is_turning: bool, is_terminal: bool) -> Tile {
        match (is_turning, is_terminal) {
            (true, true) => Tile::TurningTerminal(notes.next(), notes.next()),
            (true, false) => Tile::TurningNode(notes.next()),
            (false, true) => Tile::Terminal(notes.next(), notes.next(), notes.next()),
            (false, false) => Tile::Node(notes.next(), notes.next()),
        }
    }

    fn get_block(&self, layout_index: u8, repeater_facing: &'static str) -> GenericBlockState {
        // The repeater facing direction is reversed.
        match (self, layout_index) {
            // main straight track
            (Self::Delay(_), 0) => chain_block(),
            (Self::Delay(delay), 1) => repeater(delay.to_string(), repeater_facing),
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
            // turning variants
            (Self::TurningDelay(_), 0 | 3) => chain_block(),
            (Self::TurningDelay(_), 1) => redstone_wire(),
            (Self::TurningDelay(delay), 4) => repeater(delay.to_string(), repeater_facing),
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
        buckets[(game_tick.rem_euclid(2)) as usize]
            .entry(game_tick / 2)
            .or_default()
            .extend(notes);
    }
    buckets.into_iter().map(move |m| (m, coarse))
}
