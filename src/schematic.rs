//! Generate Minecraft litematic projections from NBS songs.

use crate::{Index, Note};
use mcdata::{BlockState, GenericBlockState, util::BlockPos};
use rustmatica::{Litematic, Region};
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};

/// 3×2 block group
pub enum Group {
    /// first, second
    DelayOnly(u8, u8),
    /// delay, center, left, right
    Delayed(u8, Option<Note>, Option<Note>, Option<Note>),
    /// left, right
    Sustain(Option<Note>, Option<Note>),
    /// center, left, right
    SustainEnd(Option<Note>, Option<Note>, Option<Note>),
}

impl Group {
    /// place 12 blocks of this group at anchor
    pub fn place(
        &self,
        region: &mut Region<GenericBlockState>,
        anchor: BlockPos,
        pointing_south: bool,
    ) {
        // [(X, Y, Z, idx)]
        const LAYOUT: [(i32, i32, i32, u8); 12] = [
            // pillar
            (1, 0, 0, 0),
            (1, 1, 0, 1),
            (1, 2, 0, 2),
            // center
            (1, 0, 1, 3),
            (1, 1, 1, 4),
            (1, 2, 1, 5),
            // left
            (0, 0, 1, 6),
            (0, 1, 1, 7),
            (0, 2, 1, 8),
            // right
            (2, 0, 1, 9),
            (2, 1, 1, 10),
            (2, 2, 1, 11),
        ];

        let facing: Cow<'static, str> = match pointing_south {
            true => "north".into(),
            false => "south".into(),
        };

        for (dx, dy, dz, idx) in &LAYOUT {
            let world_pos = match pointing_south {
                true => BlockPos::new(anchor.x + dx, anchor.y + dy, anchor.z + dz),
                false => BlockPos::new(anchor.x + dx, anchor.y + dy, anchor.z + 1 - dz),
            };
            region.set_block(world_pos, self.get_block(idx, facing.clone()));
        }
    }

    /// get block at layout index
    pub fn get_block(&self, index: &u8, facing: Cow<'static, str>) -> GenericBlockState {
        let repeater = |delay: &u8| GenericBlockState {
            name: "minecraft:repeater".into(),
            properties: HashMap::from([
                ("delay".into(), delay.to_string().into()),
                ("facing".into(), facing.clone()),
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
                0 | 3 => Self::chain_block(),
                1 => repeater(first),
                4 => repeater(second),
                _ => GenericBlockState::air(),
            },
            Group::Delayed(delay, center, left, right) => match index {
                0 => Self::chain_block(),
                1 => repeater(delay),
                3 => instrument_block(center, Self::chain_block),
                4 => note_block(center, Self::chain_block),
                6 => instrument_block(left, GenericBlockState::air),
                7 => note_block(left, GenericBlockState::air),
                9 => instrument_block(right, GenericBlockState::air),
                10 => note_block(right, GenericBlockState::air),
                _ => GenericBlockState::air(),
            },
            Group::Sustain(left, right) => match index {
                0 | 3 | 4 => Self::chain_block(),
                1 | 5 => Self::redstone_wire(),
                6 => instrument_block(left, GenericBlockState::air),
                7 => note_block(left, GenericBlockState::air),
                9 => instrument_block(right, GenericBlockState::air),
                10 => note_block(right, GenericBlockState::air),
                _ => GenericBlockState::air(),
            },
            Group::SustainEnd(center, left, right) => match index {
                0 => Self::chain_block(),
                1 => Self::redstone_wire(),
                3 => instrument_block(center, Self::chain_block),
                4 => note_block(center, Self::chain_block),
                6 => instrument_block(left, GenericBlockState::air),
                7 => note_block(left, GenericBlockState::air),
                9 => instrument_block(right, GenericBlockState::air),
                10 => note_block(right, GenericBlockState::air),
                _ => GenericBlockState::air(),
            },
        }
    }

    pub fn chain_block() -> GenericBlockState {
        GenericBlockState {
            name: "minecraft:smooth_stone".into(),
            properties: Default::default(),
        }
    }

    pub fn redstone_wire() -> GenericBlockState {
        let properties = HashMap::from([
            ("power".into(), "0".into()),
            ("north".into(), "side".into()),
            ("south".into(), "side".into()),
            ("east".into(), "side".into()),
            ("west".into(), "side".into()),
        ]);
        GenericBlockState {
            name: "minecraft:redstone_wire".into(),
            properties,
        }
    }
}

/// litematic builder
pub struct SchematicBuilder {
    /// raw track data grouped by tick: (`[({tick: [note]}, coarse)]`)
    tracks: Vec<(BTreeMap<Index, Vec<Note>>, Index)>,
    /// max groups per row (`group * wrap_length = row length`)
    wrap_length: usize,
}

impl SchematicBuilder {
    /// new builder with defaults
    pub fn new() -> Self {
        Self {
            tracks: Default::default(),
            wrap_length: usize::MAX,
        }
    }

    /// set max groups per row
    pub fn with_wrap_length(mut self, wrap_length: usize) -> Self {
        self.wrap_length = wrap_length;
        self
    }

    /// add a track with coarse delay
    pub fn add_track<I>(&mut self, track: I, coarse: Index)
    where
        I: IntoIterator<Item = (Index, Note)>,
    {
        let mut timed_notes: BTreeMap<Index, Vec<Note>> = BTreeMap::new();
        for (tick, note) in track {
            timed_notes.entry(tick).or_default().push(note);
        }

        self.tracks.push((timed_notes, coarse));
    }

    fn generate_groups(
        timed_notes: BTreeMap<Index, Vec<Note>>,
        coarse: Index,
        wrap_length: usize,
    ) -> Vec<Group> {
        let mut groups: Vec<Group> = Default::default();
        let mut current_tick: Index = Index::MAX;

        for (tick, mut notes) in timed_notes {
            let mut delay = tick.wrapping_sub(current_tick);
            let mut remaining = notes.len();

            current_tick = tick;

            // pure delay groups
            let mut carry = false;
            while let Some((group, consumed)) =
                Self::pop_delay_group(delay, coarse, carry, (groups.len() + 1) % wrap_length == 0)
            {
                groups.push(group);
                delay -= consumed;
                carry = consumed > coarse;
            }

            // terminal group
            groups.push(Group::Delayed(
                delay as _,
                notes.pop(),
                notes.pop(),
                notes.pop(),
            ));
            remaining = remaining.saturating_sub(3);

            // sustain group
            if remaining > 0 {
                groups.push(Group::Sustain(notes.pop(), notes.pop()));
                remaining = remaining.saturating_sub(2);
            }
            while remaining > 3 || remaining > 0 && (groups.len() + 1) % wrap_length == 0 {
                groups.push(Group::Sustain(notes.pop(), notes.pop()));
                remaining = remaining.saturating_sub(2);
            }
            if remaining > 0 {
                groups.push(Group::SustainEnd(notes.pop(), notes.pop(), notes.pop()));
            }
        }

        groups
    }

    fn pop_delay_group(
        delay: Index,
        coarse: Index,
        carry: bool,
        wrap: bool,
    ) -> Option<(Group, Index)> {
        // handle JE micro-timing
        if (2..=4).contains(&coarse) {
            if delay > coarse * 2 && !wrap {
                // delay chain (exit unsafe)
                let delay = coarse * 2;
                Some((Group::DelayOnly(coarse as _, coarse as _), delay))
            } else if delay > coarse * 2 {
                // delay chain at wrap (exit safe)
                let delay = coarse * 2 - 1;
                Some((Group::DelayOnly(coarse as _, (coarse - 1) as _), delay))
            } else if delay == coarse * 2 && carry {
                // drain delay chain (exit safe)
                let delay = coarse + 1;
                Some((Group::DelayOnly(coarse as _, 1), delay))
            } else if delay >= coarse && carry {
                // drain delay chain (exit safe)
                let delay = coarse - 1;
                Some((Group::Delayed(delay as _, None, None, None), delay))
            } else if delay > coarse && !carry {
                // short delay (exit safe)
                Some((Group::Delayed(coarse as _, None, None, None), coarse))
            } else {
                None
            }
        } else if coarse == 1 {
            if delay > coarse {
                Some((Group::Delayed(coarse as _, None, None, None), coarse))
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

    /// build final litematic
    pub fn build(
        self,
        description: impl Into<Cow<'static, str>>,
        author: impl Into<Cow<'static, str>>,
    ) -> Litematic {
        // destructure to avoid partial move issues
        let SchematicBuilder {
            tracks,
            wrap_length,
        } = self;

        // convert raw tracks to groups
        let track_groups: Vec<Vec<Group>> = tracks
            .into_iter()
            .map(|(timed_notes, coarse)| Self::generate_groups(timed_notes, coarse, wrap_length))
            .collect();

        let width: i32 = track_groups
            .iter()
            .map(|t| t.len().div_ceil(wrap_length) * 2 + 1)
            .sum::<usize>() as _;
        let length: i32 = wrap_length as i32 * 2 + 2;
        const HEIGHT: i32 = 4;

        let mut region: Region<GenericBlockState> = Region::new(
            "Note Block Track Schematic",
            BlockPos::new(0, 0, 0),
            BlockPos::new(width, HEIGHT, length),
        );

        // floor
        let floor_block = || GenericBlockState {
            name: "minecraft:white_concrete".into(),
            properties: Default::default(),
        };
        for (x, z) in (0..width).flat_map(|x| (0..length).map(move |z| (x, z))) {
            region.set_block(BlockPos::new(x, 0, z), floor_block());
        }

        let mut cursor: i32 = -3;
        let mut pointing_south: bool = true;

        for (index, group) in track_groups.iter().flat_map(|track| {
            // [0, 1, 2, ..., 0, 1, 2, ...]
            track.iter().enumerate()
        }) {
            let offset = (index % wrap_length) as i32;

            if index == 0 {
                // track changed
                pointing_south = true;
                cursor += 3;
            } else if offset == 0 {
                // line changed
                pointing_south = !pointing_south;
                cursor += 2;
                // turning
                let turning_anchor = Self::turning_pos(pointing_south, cursor, wrap_length * 2);
                Self::place_turning(&mut region, turning_anchor);
            }

            // track
            let anchor = match pointing_south {
                true => BlockPos::new(cursor, 1, offset * 2 + 1),
                false => BlockPos::new(cursor, 1, (wrap_length as i32 - offset) * 2 - 1),
            };

            group.place(&mut region, anchor, pointing_south);
        }

        region.as_litematic(description, author)
    }

    fn turning_pos(pointing_south: bool, cursor: i32, length: usize) -> BlockPos {
        match pointing_south {
            true => BlockPos::new(cursor - 1, 1, 0),
            false => BlockPos::new(cursor - 1, 1, length as i32 + 1),
        }
    }

    /// place turning blocks at line wrap
    fn place_turning(region: &mut Region<GenericBlockState>, anchor: BlockPos) {
        let chain_block = Group::chain_block;
        let redstone_wire = Group::redstone_wire;
        let mut place = |dx: i32, dy: i32, block: GenericBlockState| {
            region.set_block(BlockPos::new(anchor.x + dx, anchor.y + dy, anchor.z), block)
        };
        for dx in 0..=2 {
            place(dx, 0, chain_block());
            place(dx, 1, redstone_wire());
        }
    }
}
