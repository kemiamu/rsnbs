//! Tapped delay line layout for NBS song projection.
//!
//! The overall structure consists of a **control unit** (west) and a **playing unit** (east),
//! aligned north and connected block-to-block. Each row (one per TEC) pairs a [`TapLine`]
//! (the tapped delay line providing hardcoded tap delays) with a [`WithFloor<CompactLayout>`]
//! (the note block playing area).

use super::air;
use super::{Arranged, Axis, CompactLayout, EdgeArranged, Layout, Mask, WithFloor};
use crate::note::Note;
use crate::types::RedStoneTick;
use crate::util::TransEqClass;
use mcdata::{GenericBlockState, util::BlockPos};
use std::collections::BTreeMap;
use std::num::NonZero;

// TappedLayout
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Full tapped-delay-line schematic combining control and playing units.
///
/// Layout (top view, north up):
///
/// ```text
///         north
///         ↑
///   west  ┌───────────┬───────────┐  east
///         │  Control  │  Playing  │
///         │ TapLine i │ Compact i │
///         │    ⋮     │    ⋮     │
///         │ TapLine 0 │ Compact 0 │
///         └───────────┴───────────┘
///         south
/// ```
///
/// The control unit sits on the west, aligned eastward; the playing unit sits on the east,
/// each row connected via shared north (Z = 0) alignment.
pub struct TappedLayout {
    control: EdgeArranged<TapLine>,
    playing: Arranged<WithFloor<CompactLayout>>,
    size: BlockPos,
}

impl TappedLayout {
    /// Build the composite layout from TEC data.
    ///
    /// Each TEC's offsets drive the tapped delay line (control unit);
    /// its arithmetic kernel (`tec.prune()`) drives the playing unit.
    pub fn new(tecs: impl IntoIterator<Item = TransEqClass>) -> Self {
        // Decompose each TEC into delays (tap line) and kernel (playing unit).
        let mut tap_lines = Vec::new();
        let mut layouts = Vec::new();

        for tec in tecs {
            let (offsets, kernel) = tec.into_pruned();
            let notes = {
                let mut map: BTreeMap<RedStoneTick, Vec<Note>> = BTreeMap::new();
                for ((tick, tone), count) in kernel {
                    map.entry(tick)
                        .or_default()
                        .extend(std::iter::repeat(Note::new(tone)).take(count));
                }
                map
            };

            tap_lines.push(TapLine::new(offsets));
            layouts.push(CompactLayout::new(notes, None, None));
        }

        // Control unit: stack tap lines vertically, aligned by east edges.
        let control = EdgeArranged::new(
            tap_lines,
            Axis::Elevation,
            0,
            Mask::new(BlockPos::new(1, 0, 0)).unwrap(),
        );

        // Playing unit: stack compact layouts vertically.
        let playing = Arranged::new(
            layouts
                .into_iter()
                .map(|layout| WithFloor::new(layout, false)),
            Axis::Elevation,
            0,
        );

        // Bounding box: control on west, playing on east.
        let ctrl_size = control.size();
        let play_size = playing.size();
        let size = BlockPos::new(
            ctrl_size.x + play_size.x,
            ctrl_size.y.max(play_size.y),
            ctrl_size.z.max(play_size.z),
        );

        Self {
            control,
            playing,
            size,
        }
    }
}

impl Layout for TappedLayout {
    fn size(&self) -> BlockPos {
        self.size
    }

    fn get_block(&self, pos: BlockPos) -> GenericBlockState {
        let ctrl_w = self.control.size().x;
        if pos.x < ctrl_w {
            self.control.get_block(pos)
        } else {
            self.playing.get_block(pos - BlockPos::new(ctrl_w, 0, 0))
        }
    }
}

// TapLine
//
// ++++++++++++============++++++++++++============++++++++++++============

/// A single TEC's tapped delay line.
///
/// Each offset in the TEC corresponds to a hardcoded tap position along the delay line.
/// The delay line runs eastward; taps branch to note blocks at the appropriate offset.
pub struct TapLine {
    easting: i32,
    southing: i32,
}

impl TapLine {
    const ELEVATION: i32 = 2;

    pub fn new(delays: impl IntoIterator<Item = NonZero<RedStoneTick>>) -> Self {
        // TODO: compute easting from accumulated delay and
        //       southing from max concurrent notes per tap.
        let _ = delays;
        let easting = 0;
        let southing = 0;
        Self { easting, southing }
    }
}

impl Layout for TapLine {
    fn size(&self) -> BlockPos {
        BlockPos::new(self.easting, Self::ELEVATION, self.southing)
    }

    fn get_block(&self, _pos: BlockPos) -> GenericBlockState {
        air()
    }
}
