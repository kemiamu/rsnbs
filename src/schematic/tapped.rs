//! Tapped delay line layout for NBS song projection.

use super::air;
use super::{Arranged, Axis, CompactLayout, EdgeArranged, Layout, WithFloor};
use super::{chain_block, observer, redstone_torch, redstone_wire, repeater, wire_state};
use crate::analysis::BoundedTec;
use crate::note::{Instrument, Key, Notes, Tone};
use crate::types::{RedStoneTick, Tick};
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
    /// Build the composite layout from bounded TEC data.
    ///
    /// Each TEC's offsets drive the tapped delay line (control unit);
    /// its bounded kernel drives the playing unit.
    pub fn new<I: IntoIterator<Item = BoundedTec<Tone>>>(
        tecs: I,
        wrap_length: Option<NonZero<usize>>,
        full: bool,
    ) -> Self {
        let mut tap_lines = Vec::new();
        let mut layouts = Vec::new();

        // kernel -> compact layout
        for (lyr, tec) in tecs.into_iter().enumerate() {
            let tec = tec.into_inner();
            let repeater_coarse = tec.min_gap().and_then(|gap| NonZero::new(gap / 2));
            let notes = Notes::<RedStoneTick, Vec<Tone>>::from(tec.kernel)
                .into_iter()
                .map(|(tick, tones)| (tick + 2 * lyr as u32, tones))
                .collect::<Notes<RedStoneTick, Vec<Tone>>>();
            let layout = CompactLayout::new(notes, repeater_coarse, wrap_length);

            tap_lines.push(TapLine::new(tec.scatter, repeater_coarse));
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

    fn block_at(&self, pos: BlockPos) -> GenericBlockState {
        let divide = self.control.size().x;
        let side = pos.x < divide;

        let (local, size) = match side {
            true => (pos, self.control.size()),
            false => (pos - BlockPos::new(divide, 0, 0), self.playing.size()),
        };
        let dispatch = || match side {
            true => self.control.get_block(local),
            false => self.playing.get_block(local),
        };
        match local.y < size.y && local.z < size.z {
            true => dispatch(),
            false => air(),
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
        let ticks: Vec<NonZero<RedStoneTick>> = FromIterator::from_iter(ticks);
        let taps = ticks.into_iter().scan(0, |prev, tick| {
            let diff = tick.get() - *prev;
            *prev = tick.get();
            Some(Tap::new(NonZero::new(diff).unwrap(), repeater_coarse))
        });
        let delays = EdgeArranged::new(taps, Axis::Southing, 0, Axis::Easting.unit());
        let inner = delays.size();
        let width = inner.x.max(5);
        let size = BlockPos::new(width, Tap::ELEVATION, inner.z + 1);

        Self { size, delays }
    }

    fn port(&self, pos: BlockPos) -> GenericBlockState {
        let local_easting = self.size.x - pos.x - 1;
        let local_elevation = pos.y;
        let switch = self.delays.size().x == 0;
        let port_wire = || {
            let switch_south = if switch { "none" } else { "side" };
            let switch_west = if switch { "side" } else { "none" };
            wire_state("side", "none", switch_south, switch_west, "0")
        };
        let button = || {
            let tone = Tone::new(Instrument::BassDrum, Key::from_minecraft_note(0).unwrap());
            tone.note_block_state().unwrap_or_else(chain_block)
        };

        match (local_easting, local_elevation) {
            (1, 3) => redstone_torch(None::<&'static str>, true),
            (0, 1) | (1, 2) => chain_block(),
            (0, 2) => port_wire(),
            (1..=4, 1) if switch => chain_block(),
            (2, 2) if switch => repeater("4", "west", false),
            (3, 2) if switch => observer("west"),
            (4, 2) if switch => button(),
            (1, 0) if !switch => chain_block(),
            (1, 1) if !switch => redstone_torch(None::<&'static str>, false),
            _ => air(),
        }
    }
}

impl Layout for TapLine {
    fn size(&self) -> BlockPos {
        self.size
    }

    fn block_at(&self, pos: BlockPos) -> GenericBlockState {
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
    delay: NonZero<RedStoneTick>,
    size: BlockPos,
}

impl Tap {
    const ELEVATION: i32 = 4;
    const SOUTHING: i32 = 2;

    fn new(delay: NonZero<RedStoneTick>, repeater_coarse: Option<NonZero<RedStoneTick>>) -> Self {
        assert!(
            repeater_coarse.is_none_or(|c| c.get() >= 4),
            "coarse < 4 is not supported, adjust the step"
        );

        let width = (delay.get() as i32 - 6) / 8 + 4;
        let size = BlockPos::new(width, Self::ELEVATION, Self::SOUTHING);
        Self { delay, size }
    }
}

impl Layout for Tap {
    fn size(&self) -> BlockPos {
        self.size
    }

    fn block_at(&self, pos: BlockPos) -> GenericBlockState {
        let rev_easting = self.size.x - pos.x - 1;
        let delay = self.delay.get();
        let clamp = |tick: Tick| tick.min(4).to_string();
        let cycle = |off: Tick, modu: Tick| clamp((delay + off) % modu);
        let decay = |sub: Tick| clamp(delay.saturating_sub(sub));
        let phase = || (delay + 3) % 8;

        match (rev_easting, pos.x, pos.z, pos.y) {
            (_, _, _, 1) | (_, 0, 0, 2) | (1, _, 1, 2) => chain_block(),
            (_, 0, 1, 2) => redstone_torch(Some("south"), false),
            (1, _, 0, 2) => redstone_torch(Some("south"), true),
            (3.., 2.., 0, 2) => repeater("4", "east", true),
            (2.., 2.., 1, 2) => repeater("4", "west", false),
            (2, 1, 1, 2) => repeater(decay(6), "west", false),
            (3.., 1, 1, 2) => repeater(cycle(3, 8), "west", false),
            (2, 1, 0, 2) if delay < 11 => redstone_wire(),
            (2, 1, 0, 2) => repeater(decay(10), "west", false),
            (2, 2.., 0, 2) if phase() < 4 => wire_state("side", "none", "none", "side", "15"),
            (2, 2.., 0, 2) => repeater(cycle(3, 8), "east", true),
            (3.., 1, 0, 2) => repeater("3", "east", true),
            (0, _, 1, 2) => wire_state("none", "side", "side", "none", "0"),
            (0, _, 0, 2) => repeater(decay(3), "south", false),
            _ => air(),
        }
    }
}
