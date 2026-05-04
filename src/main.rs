use mcdata::{BlockState, GenericBlockState, util::BlockPos};
use rsnbs::*;
use rustmatica::Region;
use std::collections::{BTreeMap, HashMap};
use std::iter::repeat;

fn main() {
    work();
}

// fn test() {
//     let schem: Litematic =
//         Litematic::read_file("evil_cat_world_ruling_scheme/test.litematic").unwrap();

//     println!("{:?}", schem.regions[0]);
// }

/// 与世界环境保持线性关系的单个结构体
pub enum Group {
    // NOTE: 属于优化，delay_only 可能导致 JE 在特定粒度出现微时序问题
    /// delay = coarse * 2
    DelayOnly,
    /// delay <= coarse | center | left | right
    Delayed(Index, Option<Note>, Option<Note>, Option<Note>),
    /// left | right
    Sustain(Option<Note>, Option<Note>),
    // NOTE: 属于优化
    /// center | left | right
    SustainEnd(Option<Note>, Option<Note>, Option<Note>),
}

impl Group {
    // blocks
    //
    //

    /// Returns an air block state.
    fn air() -> GenericBlockState {
        GenericBlockState::air()
    }

    /// Returns a white concrete block state.
    fn floor_block() -> GenericBlockState {
        GenericBlockState {
            name: "minecraft:white_concrete".into(),
            properties: HashMap::new(),
        }
    }

    /// Returns a light blue concrete block state.
    fn chain_block() -> GenericBlockState {
        GenericBlockState {
            name: "minecraft:smooth_stone".into(),
            properties: HashMap::new(),
        }
    }

    /// Returns a redstone wire block state.
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

    /// Returns a repeater block state.
    fn repeater(delay: u8) -> GenericBlockState {
        let properties = HashMap::from([
            ("delay".into(), delay.to_string().into()),
            ("facing".into(), "north".into()),
            ("locked".into(), "false".into()),
            ("powered".into(), "false".into()),
        ]);
        GenericBlockState {
            name: "minecraft:repeater".into(),
            properties,
        }
    }

    /// Returns the note block state for an optional note, or the fallback.
    fn note_block_or_else<F>(note: &Option<Note>, fallback: F) -> GenericBlockState
    where
        F: FnOnce() -> GenericBlockState,
    {
        note.as_ref()
            .and_then(|n| n.note_block_state())
            .unwrap_or_else(fallback)
    }

    /// Returns the instrument block for an optional note, or the fallback.
    fn instrument_block_or_else<F>(note: &Option<Note>, fallback: F) -> GenericBlockState
    where
        F: FnOnce() -> GenericBlockState,
    {
        note.as_ref()
            .and_then(|n| n.instrument.instrument_block())
            .unwrap_or_else(fallback)
    }

    // layout
    //
    //

    /// Builds the layout for `DelayOnly`
    fn delay_only_layout(pos: usize) -> GenericBlockState {
        match pos {
            0 | 1 | 2 | 12 | 13 | 14 => Self::floor_block(),
            4 | 16 => Self::chain_block(),
            7 | 19 => Self::repeater(4),
            _ => Self::air(),
        }
    }

    /// Builds the layout for `Delayed`
    fn delayed_layout(
        pos: usize,
        delay: u8,
        center: &Option<Note>,
        left: &Option<Note>,
        right: &Option<Note>,
    ) -> GenericBlockState {
        match pos {
            0 | 1 | 2 | 12 | 13 | 14 => Self::floor_block(),
            4 => Self::chain_block(),
            7 => Self::repeater(delay),
            15 => Self::instrument_block_or_else(left, Self::air),
            16 => Self::instrument_block_or_else(center, Self::chain_block),
            17 => Self::instrument_block_or_else(right, Self::air),
            18 => Self::note_block_or_else(left, Self::air),
            19 => Self::note_block_or_else(center, Self::chain_block),
            20 => Self::note_block_or_else(right, Self::air),
            _ => Self::air(),
        }
    }

    /// Builds the layout for `Sustain`
    fn sustain_layout(pos: usize, left: &Option<Note>, right: &Option<Note>) -> GenericBlockState {
        match pos {
            0 | 1 | 2 | 12 | 13 | 14 => Self::floor_block(),
            4 => Self::chain_block(),
            7 => Self::redstone_wire(),
            15 => Self::instrument_block_or_else(left, Self::air),
            16 => Self::chain_block(),
            17 => Self::instrument_block_or_else(right, Self::air),
            18 => Self::note_block_or_else(left, Self::air),
            19 => Self::chain_block(),
            20 => Self::note_block_or_else(right, Self::air),
            22 => Self::redstone_wire(),
            _ => Self::air(),
        }
    }

    /// Builds the layout for `SustainEnd`
    fn sustain_end_layout(
        pos: usize,
        center: &Option<Note>,
        left: &Option<Note>,
        right: &Option<Note>,
    ) -> GenericBlockState {
        match pos {
            0 | 1 | 2 | 12 | 13 | 14 => Self::floor_block(),
            4 => Self::chain_block(),
            7 => Self::redstone_wire(),
            15 => Self::instrument_block_or_else(left, Self::air),
            16 => Self::instrument_block_or_else(center, Self::chain_block),
            17 => Self::instrument_block_or_else(right, Self::air),
            18 => Self::note_block_or_else(left, Self::air),
            19 => Self::note_block_or_else(center, Self::chain_block),
            20 => Self::note_block_or_else(right, Self::air),
            _ => Self::air(),
        }
    }

    /// Returns the block state at the given position within this group's 24-block layout.
    pub fn get_block(&self, pos: usize) -> GenericBlockState {
        match self {
            Group::DelayOnly => Self::delay_only_layout(pos),
            Group::Delayed(delay, center, left, right) => {
                Self::delayed_layout(pos, *delay as u8, center, left, right)
            }
            Group::Sustain(left, right) => Self::sustain_layout(pos, left, right),
            Group::SustainEnd(center, left, right) => {
                Self::sustain_end_layout(pos, center, left, right)
            }
        }
    }
}

pub struct Track(Vec<Group>);

impl Track {
    pub fn new(cluster: BTreeMap<Position, Note>, coarse: Index) -> Self {
        let mut timed_notes: BTreeMap<Index, Vec<Note>> = BTreeMap::new();
        for (pos, note) in cluster {
            timed_notes.entry(pos.tick()).or_default().push(note);
        }

        let mut groups: Vec<Group> = Default::default();
        let mut current_tick: Option<Index> = Default::default();

        for (tick, mut notes) in timed_notes {
            let mut delay = current_tick.map_or(tick + 1, |t| tick - t);
            let mut remaining = notes.len();

            // 纯延迟组
            while delay > coarse * 2 {
                groups.push(Group::DelayOnly);
                delay -= coarse * 2;
            }
            if delay > coarse {
                groups.push(Group::Delayed(coarse, None, None, None));
                delay -= coarse;
            }

            // 终端组
            groups.push(Group::Delayed(delay, notes.pop(), notes.pop(), notes.pop()));
            remaining = remaining.saturating_sub(3);

            // 牵引组
            if remaining > 0 {
                groups.push(Group::Sustain(notes.pop(), notes.pop()));
                remaining = remaining.saturating_sub(2);
            }

            // 重复牵引或结束
            while remaining > 0 {
                if remaining <= 3 {
                    groups.push(Group::SustainEnd(notes.pop(), notes.pop(), notes.pop()));
                    remaining = 0;
                } else {
                    groups.push(Group::Sustain(notes.pop(), notes.pop()));
                    remaining -= 2;
                }
            }

            current_tick = Some(tick)
        }
        Track(groups)
    }

    pub fn len(&self) -> usize {
        self.0.len() * 2
    }
}

const GROUP_VOLUME: usize = 24;

pub struct TrackIterator<'a> {
    /// Groups magnified by 24 times
    mapping: std::iter::FlatMap<
        std::slice::Iter<'a, Group>,
        std::iter::Take<std::iter::Repeat<&'a Group>>,
        fn(&'a Group) -> std::iter::Take<std::iter::Repeat<&'a Group>>,
    >,
    /// Total number of iterations
    index: usize,
}

impl<'a> TrackIterator<'a> {
    pub fn new(track: &'a Track) -> Self {
        Self {
            mapping: track.0.iter().flat_map(|g| repeat(g).take(GROUP_VOLUME)),
            index: 0,
        }
    }
}

impl<'a> Iterator for TrackIterator<'a> {
    type Item = (BlockPos, GenericBlockState);

    fn next(&mut self) -> Option<Self::Item> {
        let group = self.mapping.next()?;
        // 扫描编号
        let pos = self.index % GROUP_VOLUME;
        // 轨道局部坐标映射
        let block_pos = BlockPos::new(
            (self.index % 3) as _,
            ((self.index / 3) % 4) as _,
            (self.index / 12) as _,
        );
        self.index += 1;
        Some((block_pos, group.get_block(pos)))
    }
}

fn work() {
    let song = Song::open_nbs("evil_cat_world_ruling_scheme/source.nbs").unwrap();
    let mut notes = song.notes;

    // 参数

    let patterns = rsnbs::PATTERNS;
    let song_length: Index = notes.iter().map(|(p, _)| p.tick()).max().unwrap() + 1;
    let coarse: Index = 4;

    // 按照匹配规则分簇

    let mut clusters: Vec<BTreeMap<Position, Note>> = Default::default();
    for &pattern in patterns {
        let (matched, unmatched) =
            notes.matches_by(pattern, song_length, |a, b| a.tone() == b.tone());

        clusters.push(matched.clone());
        notes = unmatched;
        // notes.append(&mut matched);
        // notes.sort();
    }

    // 生成结构体

    let tracks: Vec<Track> = clusters
        .into_iter()
        .map(|cluster| Track::new(cluster, coarse))
        .collect();

    // 根据结构体生成原理图

    let mut region: Region<GenericBlockState> = Region::new(
        "Planet",
        BlockPos::new(0, 0, 0),
        BlockPos::new(
            (tracks.len() * 3) as _,
            4,
            (tracks.iter().map(Track::len).max().unwrap()) as _,
        ),
    );

    // 填充结构体到原理图
    for (track_idx, track) in tracks.iter().enumerate() {
        let iter = TrackIterator::new(track);
        for (block_pos, block_state) in iter {
            let x_offset = track_idx as i32 * 3;
            let world_pos = BlockPos::new(x_offset + block_pos.x, block_pos.y, block_pos.z);
            region.set_block(world_pos, block_state);
        }
    }

    // 保存原理图
    let planet = region.as_litematic(
        "Generated from evil_cat_world_ruling_scheme/source.nbs",
        "Planet",
    );
    planet
        .write_file("evil_cat_world_ruling_scheme/generated.litematic")
        .unwrap();
}
