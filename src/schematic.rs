//! Generate Minecraft litematic projections from NBS songs.

use crate::{Index, Note};
use mcdata::{BlockState, GenericBlockState, util::BlockPos};
use rustmatica::{Litematic, Region};
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};

/// 3×2 block group
pub enum Group {
    /// no notes, just delay
    DelayOnly(u8, u8),
    /// terminal: delay + up to 3 notes
    Delayed(u8, Option<Note>, Option<Note>, Option<Note>),
    /// sustain up to 2 notes
    Sustain(Option<Note>, Option<Note>),
    /// final: sustain up to 3 notes
    SustainEnd(Option<Note>, Option<Note>, Option<Note>),
}

/// litematic builder
pub struct SchematicBuilder {
    tracks: Vec<Vec<Group>>,
    wrap_length: usize,
    floor_block: GenericBlockState,
    chain_block: GenericBlockState,
}

impl SchematicBuilder {
    /// new builder with defaults
    pub fn new() -> Self {
        let floor_block = GenericBlockState {
            name: "minecraft:white_concrete".into(),
            properties: Default::default(),
        };
        let chain_block = GenericBlockState {
            name: "minecraft:smooth_stone".into(),
            properties: Default::default(),
        };
        Self {
            tracks: Default::default(),
            wrap_length: usize::MAX,
            floor_block,
            chain_block,
        }
    }

    /// set max groups per row
    pub fn with_wrap_length(mut self, wrap_length: usize) -> Self {
        self.wrap_length = wrap_length;
        self
    }

    /// set floor block type
    pub fn with_floor_block(mut self, block: GenericBlockState) -> Self {
        self.floor_block = block;
        self
    }

    /// set chain block type
    pub fn with_chain_block(mut self, block: GenericBlockState) -> Self {
        self.chain_block = block;
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

        let mut groups: Vec<Group> = Default::default();
        let mut current_tick: Index = Index::MAX;

        for (tick, mut notes) in timed_notes {
            let mut delay = tick.wrapping_sub(current_tick);
            let mut remaining = notes.len();

            current_tick = tick;

            // pure delay groups
            while let Some((group, consumed)) = Self::pop_delay_group(delay, coarse) {
                groups.push(group);
                delay -= consumed;
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

            // repeat sustain or end
            while remaining > 3 {
                groups.push(Group::Sustain(notes.pop(), notes.pop()));
                remaining -= 2;
            }
            if remaining > 0 {
                groups.push(Group::SustainEnd(notes.pop(), notes.pop(), notes.pop()));
                remaining = 0;
            }
        }

        self.tracks.push(groups);
    }

    fn pop_delay_group(delay: Index, coarse: Index) -> Option<(Group, Index)> {
        // handle JE micro-timing
        if (2..=4).contains(&coarse) {
            if delay > coarse * 2 {
                Some((Group::DelayOnly(coarse as _, coarse as _), coarse * 2))
            } else if delay == coarse * 2 {
                Some((Group::DelayOnly(coarse as _, 1), coarse + 1))
            } else if delay >= coarse {
                Some((Group::Delayed(coarse as _, None, None, None), coarse - 1))
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
        let width: i32 = self.tracks.iter().map(|t| t.len() * 4 + 1).sum::<usize>() as _;
        let length: i32 = self.wrap_length as i32 * 2 + 2;
        const HEIGHT: i32 = 4;

        let mut region: Region<GenericBlockState> = Region::new(
            "Note Block Track Schematic",
            BlockPos::new(0, 0, 0),
            BlockPos::new(width, HEIGHT, length),
        );

        // floor
        for (x, z) in (0..length).flat_map(|x| (0..width).map(move |z| (x, z))) {
            region.set_block(BlockPos::new(x, 0, z), self.floor_block.clone());
        }

        let mut cursor: i32 = -3;
        let mut pointing_north: bool = false;

        for (index, group) in self.tracks.iter().flat_map(|track| {
            // [0, 1, 2, ..., 0, 1, 2, ...]
            track.iter().enumerate()
        }) {
            // track changed
            if index == 0 {
                pointing_north = false;
                cursor += 1;
            }
            // line changed
            if index % self.wrap_length == 0 {
                pointing_north = !pointing_north;
                cursor += 2;
            }

            let progress = (index % self.wrap_length) as i32;
            let anchor = match pointing_north {
                true => BlockPos::new(cursor, 1, progress * 2 + 1),
                false => BlockPos::new(cursor, 1, (self.wrap_length as i32 - progress) * 2 - 1),
            };

            self.place_group(&mut region, group, anchor, pointing_north);
        }

        region.as_litematic(description, author)
    }

    /// place 12 blocks of a group at anchor
    fn place_group(
        &self,
        region: &mut Region<GenericBlockState>,
        group: &Group,
        anchor: BlockPos,
        pointing_north: bool,
    ) {
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

        // idk why mojang does this :(
        let facing: Cow<'static, str> = match pointing_north {
            true => "south".into(),
            false => "north".into(),
        };

        for (dx, dy, dz, idx) in &LAYOUT {
            let world_pos = match pointing_north {
                true => BlockPos::new(anchor.x + dx, anchor.y + dy, anchor.z + dz),
                false => BlockPos::new(anchor.x + dx, anchor.y + dy, anchor.z + 1 - dz),
            };
            region.set_block(world_pos, self.get_block(group, idx, facing.clone()));
        }
    }

    fn get_block(&self, group: &Group, index: &u8, facing: Cow<'static, str>) -> GenericBlockState {
        match group {
            Group::DelayOnly(first, second) => match index {
                0 | 3 => self.chain_block.clone(),
                1 => Self::repeater(first, facing),
                4 => Self::repeater(second, facing),
                _ => GenericBlockState::air(),
            },
            Group::Delayed(delay, center, left, right) => match index {
                0 => self.chain_block.clone(),
                1 => Self::repeater(delay, facing),
                3 => Self::instrument_block_or_else(center, || self.chain_block.clone()),
                4 => Self::note_block_or_else(center, || self.chain_block.clone()),
                6 => Self::instrument_block_or_else(left, || GenericBlockState::air()),
                7 => Self::note_block_or_else(left, || GenericBlockState::air()),
                9 => Self::instrument_block_or_else(right, || GenericBlockState::air()),
                10 => Self::note_block_or_else(right, || GenericBlockState::air()),
                _ => GenericBlockState::air(),
            },
            Group::Sustain(left, right) => match index {
                0 | 3 | 4 => self.chain_block.clone(),
                1 | 5 => Self::redstone_wire(),
                6 => Self::instrument_block_or_else(left, || GenericBlockState::air()),
                7 => Self::note_block_or_else(left, || GenericBlockState::air()),
                9 => Self::instrument_block_or_else(right, || GenericBlockState::air()),
                10 => Self::note_block_or_else(right, || GenericBlockState::air()),
                _ => GenericBlockState::air(),
            },
            Group::SustainEnd(center, left, right) => match index {
                0 => self.chain_block.clone(),
                1 => Self::redstone_wire(),
                3 => Self::instrument_block_or_else(center, || self.chain_block.clone()),
                4 => Self::note_block_or_else(center, || self.chain_block.clone()),
                6 => Self::instrument_block_or_else(left, || GenericBlockState::air()),
                7 => Self::note_block_or_else(left, || GenericBlockState::air()),
                9 => Self::instrument_block_or_else(right, || GenericBlockState::air()),
                10 => Self::note_block_or_else(right, || GenericBlockState::air()),
                _ => GenericBlockState::air(),
            },
        }
    }

    fn redstone_wire() -> GenericBlockState {
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

    fn repeater(delay: &u8, facing: Cow<'static, str>) -> GenericBlockState {
        let properties = HashMap::from([
            ("delay".into(), delay.to_string().into()),
            ("facing".into(), facing),
            ("locked".into(), "false".into()),
            ("powered".into(), "false".into()),
        ]);
        GenericBlockState {
            name: "minecraft:repeater".into(),
            properties,
        }
    }

    fn note_block_or_else<F>(note: &Option<Note>, fallback: F) -> GenericBlockState
    where
        F: FnOnce() -> GenericBlockState,
    {
        note.as_ref()
            .and_then(|n| n.note_block_state())
            .unwrap_or_else(fallback)
    }

    fn instrument_block_or_else<F>(note: &Option<Note>, fallback: F) -> GenericBlockState
    where
        F: FnOnce() -> GenericBlockState,
    {
        note.as_ref()
            .and_then(|n| n.instrument.instrument_block())
            .unwrap_or_else(fallback)
    }
}
