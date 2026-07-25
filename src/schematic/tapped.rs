//! Tapped delay line layout for NBS song projection.

use super::air;
use super::{Arranged, Axis, CompactLayout, EdgeArranged, Layout, Mask, WithFloor};
use crate::note::{Note, Notes, Tone};
use crate::types::RedStoneTick;
use crate::util::TransEqClass;
use mcdata::{GenericBlockState, util::BlockPos};
use std::collections::BTreeMap;
use std::num::NonZero;

// TappedLayout
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Full tapped-delay-line schematic combining control and playing units.
pub struct TappedLayout {
    control: EdgeArranged<TapLine>,
    playing: Arranged<WithFloor<CompactLayout>>,
    size: BlockPos,
}

impl TappedLayout {
    /// Build the composite layout from TEC data.
    ///
    /// Each TEC's offsets drive the tapped delay line (control unit);
    /// its arithmetic kernel (`tec.into_pruned()`) drives the playing unit.
    pub fn new<I: IntoIterator<Item = TransEqClass>>(
        tecs: I,
        wrap_length: Option<NonZero<usize>>,
        full: bool,
    ) -> Self {
        let mut tap_lines = Vec::new();
        let mut layouts = Vec::new();

        for tec in tecs {
            let (offsets, kernel) = tec.into_pruned();

            // kernel -> compact layout
            let notes: Notes<RedStoneTick, Vec<Tone>> = kernel.into();
            let repeater_coarse = std::iter::once(0)
                .chain(offsets.iter().map(|o| o.get()))
                .zip(offsets.iter().map(|o| o.get()))
                .map(|(prev, next)| next - prev)
                .min()
                .and_then(|gap| NonZero::new(gap / 2));
            let layout = CompactLayout::new(notes, repeater_coarse, wrap_length);

            tap_lines.push(TapLine::new(offsets));
            layouts.push(WithFloor::new(layout, full));
        }

        // arrange both sides
        let control = EdgeArranged::new(tap_lines, Axis::Elevation, 0, Axis::Easting.unit());
        let playing = Arranged::new(layouts, Axis::Elevation, 0);

        // composite bounding box
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
        match pos.x < ctrl_w {
            true => self.control.get_block(pos),
            false => self.playing.get_block(pos - BlockPos::new(ctrl_w, 0, 0)),
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

    pub fn new<I: IntoIterator<Item = NonZero<RedStoneTick>>>(delays: I) -> Self {
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
