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
        Self {
            tracks: Default::default(),
            wrap_length: usize::MAX,
            floor_block: GenericBlockState {
                name: "minecraft:white_concrete".into(),
                properties: Default::default(),
            },
            chain_block: GenericBlockState {
                name: "minecraft:smooth_stone".into(),
                properties: Default::default(),
            },
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
        let mut current_tick: Option<Index> = Default::default();

        for (tick, mut notes) in timed_notes {
            let mut delay = current_tick.map_or(tick + 1, |t| tick - t);
            let mut remaining = notes.len();

            current_tick = Some(tick);

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
        // T-shape interlocking: a single pass is 3 wide, two passes overlap by 1,
        // so n passes span `1 + 2n` in x. One row = forward + backward = 2 passes,
        // thus `1 + 4*wrap_length` per row. Total = `n_groups * 4 + 1`.
        let mut wrapped_tracks: Vec<Vec<&[Group]>> = Default::default();
        for track in &self.tracks {
            wrapped_tracks.push(track.chunks(self.wrap_length).collect());
        }

        let width: i32 = self.tracks.iter().map(|t| t.len() * 4 + 1).sum::<usize>() as _;
        const HEIGHT: i32 = 4;

        let mut region: Region<GenericBlockState> = Region::new(
            "Note Block Track Schematic",
            BlockPos::new(0, 0, 0),
            BlockPos::new(width, HEIGHT, (self.wrap_length + 2) as _),
        );

        // floor
        for x in 0..width {
            for z in 0..(self.wrap_length as i32 + 2) {
                region.set_block(BlockPos::new(x, 0, z), self.floor_block.clone());
            }
        }

        let mut cursor: i32 = 0;
        let turning_blocks = [
            (BlockPos::new(1, 1, 0), &self.chain_block),
            (BlockPos::new(2, 1, 0), &self.chain_block),
            (BlockPos::new(3, 1, 0), &self.chain_block),
            (BlockPos::new(1, 2, 0), &Self::redstone_wire()),
            (BlockPos::new(2, 2, 0), &Self::redstone_wire()),
            (BlockPos::new(3, 2, 0), &Self::redstone_wire()),
        ];
        let group_blocks = [
            (BlockPos::new(1, 0, 0), 0),
            (BlockPos::new(1, 1, 0), 1),
            (BlockPos::new(1, 2, 0), 2),
            (BlockPos::new(1, 0, 1), 3),
            (BlockPos::new(1, 1, 1), 4),
            (BlockPos::new(1, 2, 1), 5),
            (BlockPos::new(0, 0, 1), 6),
            (BlockPos::new(0, 1, 1), 7),
            (BlockPos::new(0, 2, 1), 8),
            (BlockPos::new(2, 0, 1), 9),
            (BlockPos::new(2, 1, 1), 10),
            (BlockPos::new(2, 2, 1), 11),
        ];

        for (_track_idx, wrapped_track) in wrapped_tracks.iter().enumerate() {
            for (line_idx, line) in wrapped_track.iter().enumerate() {
                for (group_idx, group) in line.iter().enumerate() {
                    if group_idx * 2 < self.wrap_length {
                        for (pos, index) in group_blocks {
                            let world_pos = BlockPos::new(
                                pos.x + cursor,
                                pos.y + 1,
                                pos.z + group_idx as i32 * 2 + 1,
                            );
                            region
                                .set_block(world_pos, self.get_block(group, index, "north".into()));
                        }
                    } else {
                        for (pos, index) in group_blocks {
                            let world_pos = BlockPos::new(
                                pos.x + cursor + 2,
                                pos.y + 1,
                                (self.wrap_length as i32 - group_idx as i32) * 2 - pos.z,
                            );
                            region
                                .set_block(world_pos, self.get_block(group, index, "south".into()));
                        }
                    }
                    if group_idx * 2 == self.wrap_length {
                        for &(pos, block) in &turning_blocks {
                            let world_pos = BlockPos::new(
                                pos.x + cursor,
                                pos.y,
                                pos.z + self.wrap_length as i32 - 1,
                            );
                            region.set_block(world_pos, block.clone());
                        }
                    } else if group_idx == 0 && line_idx != 0 {
                        for &(pos, block) in &turning_blocks {
                            let world_pos = BlockPos::new(pos.x + cursor, pos.y, pos.z);
                            region.set_block(world_pos, block.clone());
                        }
                    }
                }
                cursor += 4;
            }
            cursor += 1;
        }

        region.as_litematic(description, author)
    }

    fn get_block(&self, group: &Group, index: u8, facing: Cow<'static, str>) -> GenericBlockState {
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
