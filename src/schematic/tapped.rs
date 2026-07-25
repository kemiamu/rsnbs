//! Tapped delay line layout for NBS song projection.

use super::air;
use super::{Arranged, Axis, CompactLayout, EdgeArranged, Layout, Mask, WithFloor};
use crate::note::{Note, Notes, Tone};
use crate::types::{RedStoneTick, Tick};
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

        // kernel -> compact layout
        for (lyr, tec) in tecs.into_iter().enumerate() {
            let (offsets, kernel) = tec.into_pruned();
            let notes = Notes::<RedStoneTick, Vec<Tone>>::from(kernel)
                .into_iter()
                .map(|(tick, tones)| (tick + 2 * lyr as u32, tones))
                .collect::<Notes<RedStoneTick, Vec<Tone>>>();
            let repeater_coarse = std::iter::once(0)
                .chain(offsets.iter().map(|o| o.get()))
                .zip(offsets.iter().map(|o| o.get()))
                .map(|(prev, next)| next - prev)
                .min()
                .and_then(|gap| NonZero::new(gap / 2));
            let layout = CompactLayout::new(notes, repeater_coarse, wrap_length);

            tap_lines.push(TapLine::new(offsets, repeater_coarse));
            layouts.push(WithFloor::new(layout, full));
        }

        // signal propagates upward, so lower layers need more delay
        tap_lines.reverse();
        layouts.reverse();

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
pub struct TapLine {
    delays: EdgeArranged<Tap>,
    size: BlockPos,
}

impl TapLine {
    const INNER_ANCHOR: BlockPos = BlockPos::new(1, 0, 0);

    pub fn new<I: IntoIterator<Item = NonZero<RedStoneTick>>>(
        ticks: I,
        repeater_coarse: Option<NonZero<RedStoneTick>>,
    ) -> Self {
        let taps = ticks.into_iter().scan(0, |prev, tick| {
            let diff = tick.get() - *prev;
            *prev = tick.get();
            Some(Tap::new(NonZero::new(diff).unwrap(), repeater_coarse))
        });
        let delays = EdgeArranged::new(taps, Axis::Southing, 0, Axis::Easting.unit());
        let inner = delays.size();
        let size = BlockPos::new(inner.x, Tap::ELEVATION, inner.z + 1);

        Self { size, delays }
    }
}

impl Layout for TapLine {
    fn size(&self) -> BlockPos {
        self.size
    }

    fn get_block(&self, pos: BlockPos) -> GenericBlockState {
        if pos.x == 0 {
            return air(); // TODO: spine control blocks
        }
        self.delays.get_block(pos - Self::INNER_ANCHOR)
    }
}

// Tap
//
// ++++++++++++============++++++++++++============++++++++++++============

/// A single delay element in the tapped delay line.
struct Tap {
    ticks: NonZero<RedStoneTick>,
}

impl Tap {
    const ELEVATION: i32 = 4;

    fn new(ticks: NonZero<RedStoneTick>, repeater_coarse: Option<NonZero<RedStoneTick>>) -> Self {
        assert!(
            repeater_coarse.is_none_or(|c| c.get() >= 4),
            "coarse < 4 is not supported, adjust the step"
        );
        Self { ticks }
    }
}

impl Layout for Tap {
    fn size(&self) -> BlockPos {
        // ceil(ticks / 2) blocks along Z, each repeater at 2-tick setting
        let z_len = self.ticks.get().div_ceil(2) as i32;
        BlockPos::new(2, 1, z_len)
    }

    fn get_block(&self, _pos: BlockPos) -> GenericBlockState {
        air() // TODO: place repeater chain
    }
}
