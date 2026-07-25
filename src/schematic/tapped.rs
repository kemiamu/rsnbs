//! Tapped delay line layout for NBS song projection.

use super::air;
use super::{Arranged, Axis, CompactLayout, EdgeArranged, Layout, WithFloor};
use crate::note::{Notes, Tone};
use crate::schematic::{chain_block, redstone_torch, redstone_wire, repeater};
use crate::types::RedStoneTick;
use crate::util::TransEqClass;
use mcdata::{GenericBlockState, util::BlockPos};
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
        let ctrl_width = self.control.size().x;
        let local = BlockPos::new(pos.x - ctrl_width, pos.y, pos.z);
        let ctrl_size = self.control.size();
        let play_size = self.playing.size();
        match pos.x < ctrl_width {
            true if (0..ctrl_size.y).contains(&pos.y) && (0..ctrl_size.z).contains(&pos.z) => {
                self.control.get_block(pos)
            }
            false if (0..play_size.y).contains(&local.y) && (0..play_size.z).contains(&local.z) => {
                self.playing.get_block(local)
            }
            _ => air(),
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
        let size = BlockPos::new(inner.x.max(2), Tap::ELEVATION, inner.z + 1);

        Self { size, delays }
    }

    fn port(&self, pos: BlockPos) -> GenericBlockState {
        let local_easting = self.size.x - pos.x - 1;
        let local_elevation = pos.y;

        match (local_easting, local_elevation) {
            (0, 1) | (1, 0) | (1, 2) => chain_block(),
            (1, 1) | (1, 3) => redstone_torch(None::<&'static str>, true),
            (0, 2) => redstone_wire(),
            _ => air(),
        }
    }
}

impl Layout for TapLine {
    fn size(&self) -> BlockPos {
        self.size
    }

    fn get_block(&self, pos: BlockPos) -> GenericBlockState {
        const INNER_ANCHOR: BlockPos = BlockPos::new(0, 0, 1);
        match pos.z == 0 {
            true => self.port(pos),
            false => self.delays.get_block(pos - INNER_ANCHOR),
        }
    }
}

// Tap
//
// ++++++++++++============++++++++++++============++++++++++++============

/// A single delay element in the tapped delay line.
struct Tap {
    tick: NonZero<RedStoneTick>,
    size: BlockPos,
}

impl Tap {
    const ELEVATION: i32 = 4;
    const SOUTHING: i32 = 2;

    fn new(tick: NonZero<RedStoneTick>, repeater_coarse: Option<NonZero<RedStoneTick>>) -> Self {
        assert!(
            repeater_coarse.is_none_or(|c| c.get() >= 4),
            "coarse < 4 is not supported, adjust the step"
        );

        let width = (tick.get() as i32 - 6) / 8 + 4;
        let size = BlockPos::new(width, Self::ELEVATION, Self::SOUTHING);
        Self { tick, size }
    }
}

impl Layout for Tap {
    fn size(&self) -> BlockPos {
        self.size
    }

    fn get_block(&self, pos: BlockPos) -> GenericBlockState {
        let rev_easting = self.size.x - pos.x - 1;

        match (rev_easting, pos.x, pos.z, pos.y) {
            (_, _, _, 1) | (_, 0, 0, 2) | (1, _, 1, 2) => chain_block(),
            (_, 0, 1, 2) | (1, _, 0, 2) => redstone_torch(Some("south"), true),
            (3.., 2.., 0, 2) => repeater("4", "east"),
            (2.., 2.., 1, 2) => repeater("4", "west"),
            (2, 1, 1, 2) => repeater(self.tick.get().saturating_sub(6).min(4).to_string(), "west"),
            (3.., 1, 1, 2) => repeater(((self.tick.get() + 6) % 8).min(4).to_string(), "west"),
            (2, 1, 0, 2) if self.tick.get() < 11 => redstone_wire(),
            (2, 1, 0, 2) => repeater(
                self.tick.get().saturating_sub(10).min(4).to_string(),
                "west",
            ),
            (2, 2.., 0, 2) if (self.tick.get() + 6) % 8 < 4 => redstone_wire(),
            (2, 2.., 0, 2) => repeater(((self.tick.get() + 3) % 8).min(4).to_string(), "east"),
            (3.., 1, 0, 2) => repeater("3", "east"),

            (0, _, 1, 2) => redstone_wire(),
            (0, _, 0, 2) => repeater(
                self.tick.get().saturating_sub(3).min(4).to_string(),
                "south",
            ),
            _ => air(),
        }
    }
}
