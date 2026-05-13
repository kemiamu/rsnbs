use super::*;
use mcdata::{BlockState, util::BlockPos};
use rustmatica::Region;
use std::{
    collections::{BTreeMap, HashSet},
    iter::repeat,
};

// TEST: temporary test case
/// Pre-defined patterns for note block arrangement.
pub const PATTERNS: &[&[Index]] = &[
    // &[0, 64, 128, 192, 32, 96, 160, 224],
    // &[0, 64, 128, 192],
    // &[0, 128],
    &[0, 24, 48, 72],
    &[0, 48],
    &[0],
];

#[test]
fn test_pattern_matching() {
    let mut song = Song::open_nbs("fixtures/source.nbs").unwrap();
    let mut notes = song.notes;

    let patterns = PATTERNS;
    // let song_length: Index = 144;
    let song_length: Index = notes.iter().map(|(p, _)| p.tick()).max().unwrap() + 1;

    let mut clusters: Vec<BTreeMap<Position, Note>> = Default::default();
    for &pattern in patterns {
        let (matched, unmatched) =
            notes.matches_by(pattern, song_length, |a, b| a.tone() == b.tone());

        clusters.push(matched.clone());
        notes = unmatched;
        // notes.append(&mut matched);
        // notes.sort();
    }

    song.notes = reassign_layers(clusters).into();
    song.header.is_loop = true;
    song.save_nbs("fixtures/out.nbs").unwrap();
}

#[test]
fn analyze_tones() {
    let mut song = Song::open_nbs("fixtures/source.nbs").unwrap();

    let mut by_tone: BTreeMap<Tone, Vec<(Position, Note)>> = Default::default();
    for (pos, note) in song.notes {
        by_tone.entry(note.tone()).or_default().push((pos, note));
    }
    let slices: Vec<BTreeMap<Position, Note>> = by_tone
        .into_values()
        .map(|v| v.into_iter().collect())
        .collect();

    song.notes = reassign_layers(slices);
    song.header.is_loop = true;
    song.save_nbs("fixtures/analyzed.nbs").unwrap();
}

// 按照列表重新分配层级
fn reassign_layers(slices: Vec<BTreeMap<Position, Note>>) -> BTreeMap<Position, Note> {
    let mut base_layer: Index = Default::default();
    let mut result: BTreeMap<Position, Note> = Default::default();

    for notes in slices {
        let mut current_layer: Index = Default::default();
        let mut max_layer: Index = Default::default();

        let mut notes = notes.into_iter().peekable();
        while let Some((mut pos, note)) = notes.next() {
            pos.layer = base_layer + current_layer;

            max_layer = max_layer.max(current_layer + 2);
            match notes.peek().map(|(p, _)| p.tick()) == Some(pos.tick()) {
                true => current_layer += 1,
                false => current_layer = 0,
            }

            result.insert(pos, note);
        }
        base_layer += max_layer;
    }

    result
}

#[test]
fn generating_and_load() {
    let song_path = "fixtures/test_song.nbs";

    // README.md Example: Generating and loading a song
    let mut song = Song::new();
    song.header.is_loop = true;
    for i in 0..25 {
        let pos = Position::new(i, 0);
        let key = Key::from_minecraft_note(i).unwrap();
        let note = Note::new(Instrument::Harp, key);
        song.notes.insert(pos, note);
    }
    song.save_nbs(song_path).unwrap();

    // README.md Example: Iterating over notes
    let song = Song::open_nbs(song_path).unwrap();
    for (position, note) in song.notes {
        println!("tick: {:02}, key: {}", position.tick, note.key)
    }
}

#[test]
fn test_generation_litematic() {
    let song = Song::open_nbs("fixtures/source.nbs").unwrap();
    let mut notes = song.notes;

    // 参数

    let patterns = PATTERNS;
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
    let planet = region.as_litematic("Generated from source.nbs", "Planet");
    planet.write_file("fixtures/generated.litematic").unwrap();
}

#[test]
fn test_analyze_transposition_equivalence() {
    let mut song = Song::open_nbs("fixtures/source.nbs").unwrap();

    // params
    let song_length: Index = song.notes.iter().map(|(p, _)| p.tick()).max().unwrap() + 1;
    let half_length: Index = song_length / 2; // floor division

    // Plane-form notes multiset [note: (x: tick, y: tone), ...]
    let mut notes_multiset: Vec<(Index, Tone)> = song
        .notes
        .into_iter()
        .map(|(p, n)| (p.tick(), n.tone()))
        .collect();
    notes_multiset.sort_unstable();

    // Left point of translation {offset: [note_point, ...], ...}
    let mut offsets: HashMap<Index, Vec<(Index, Tone)>> = notes_multiset
        .iter()
        .enumerate()
        .flat_map(|(i, a)| notes_multiset[..i].iter().map(move |b| (b, a)))
        .fold(Default::default(), |mut acc, ((tb, nb), (ta, na))| {
            let distance = match na == nb {
                true => ta - tb,
                false => return acc,
            };
            let offset = match distance > half_length {
                true => song_length - distance,
                false => distance,
            };
            acc.entry(offset).or_default().push((*tb, *nb));
            acc
        });

    let mut offset_counts: Vec<(usize, Index)> = offsets
        .into_iter()
        .filter(|(_, v)| v.len() > 1)
        .map(|(k, v)| (v.len(), k))
        .collect();
    offset_counts.sort_unstable();
    offset_counts.reverse();

    // TEST: This theory is unstable

    // Approximate Minkowski sum decomposition via conflict resolution for noisy data.
    //
    // Find all possible positions for the `pattern`,
    // then iteratively select the group with the least conflict.
    //
    // Suppose multiset = [0, 1, 2, 3, 3, 5, 6], pattern = [1, 3], Then:
    //
    // | 0^1 | 1^1 | 2^1 | 3^2 | 5^1 | 6^1 | ... |
    // |:---:|:---:|:---:|:---:|:---:|:---:|:---:|
    // | A0  | A1  |     | A3  |     |     |     |
    // |     | B0  | B1  |     | B3  |     |     |
    // |     |     | C0  | C1  |     | C3  |     |
    //
    // Group A overlap degree: 4 (1/1 + 2/1 + 2/2)
    // Group B overlap degree: 5 (2/1 + 2/1 + 1/1)
    // Group C overlap degree: 4 (2/1 + 2/2 + 1/1)
    let satisfy_constraints = |multiset: &[(Index, Tone)], pattern: &[(Index, Tone)]| {
        debug_assert!(!pattern.is_empty());

        // Convert multiset to a count map of elements [p^k, ...]
        let counts: HashMap<_, usize> = multiset.iter().fold(Default::default(), |mut acc, &p| {
            *acc.entry(p).or_default() += 1;
            acc
        });

        // Any positions satisfying the matching pattern
        let mut groups: HashSet<(Index, Tone)> = Default::default();

        let original_counts = counts.clone();

        // 2. 生成所有可能的组
        let first = pattern[0];
        let mut groups: Vec<Vec<(Index, Tone)>> = Vec::new();

        for &(base_tick, base_tone) in multiset {
            if base_tone != first.1 {
                continue;
            }
            let dt = base_tick as i64 - first.0 as i64;
            let mut group = Vec::with_capacity(pattern.len());
            let mut valid = true;
            for &(pt, ptone) in pattern {
                let abs_tick = (pt as i64 + dt) as Index;
                let abs_tone = ptone;
                if counts.get(&(abs_tick, abs_tone)).unwrap_or(&0) == &0 {
                    valid = false;
                    break;
                }
                group.push((abs_tick, abs_tone));
            }
            if valid {
                groups.push(group);
            }
        }
        groups.sort();
        groups.dedup();

        // 3. 贪心选择
        let mut selected_groups: Vec<Vec<(Index, Tone)>> = Vec::new();
        let mut remaining = counts;

        loop {
            // 可用组索引
            let mut available_indices = Vec::new();
            for (idx, group) in groups.iter().enumerate() {
                if group.iter().all(|p| remaining.get(p).unwrap_or(&0) > &0) {
                    available_indices.push(idx);
                }
            }
            if available_indices.is_empty() {
                break;
            }

            // 计算冲突值，选最小值
            let mut best_idx = available_indices[0];
            let mut best_conflict = f64::MAX;

            for &idx in &available_indices {
                let group = &groups[idx];

                let mut occupancy: HashMap<(Index, Tone), usize> = HashMap::new();
                for g in &selected_groups {
                    for &p in g {
                        *occupancy.entry(p).or_insert(0) += 1;
                    }
                }

                let mut conflict = 0.0;
                for &p in group {
                    let occ = occupancy.get(&p).unwrap_or(&0) + 1;
                    let cap = *original_counts.get(&p).unwrap_or(&1);
                    conflict += occ as f64 / cap as f64;
                }

                if conflict < best_conflict {
                    best_conflict = conflict;
                    best_idx = idx;
                }
            }

            let chosen = groups[best_idx].clone();
            selected_groups.push(chosen.clone());

            for &p in &chosen {
                let entry = remaining.get_mut(&p).unwrap();
                *entry -= 1;
                if *entry == 0 {
                    remaining.remove(&p);
                }
            }
        }

        // 展平所有选中的组
        let mut result = Vec::new();
        for group in selected_groups {
            result.extend(group);
        }
        result
    };

    let mut subset: Vec<(Index, Tone)> = Default::default();

    for (count, offset) in &offset_counts {}

    //
    //
    //

    // let mut offsets_rev: Vec<(Index, HashMap<Tone, Vec<usize>>)> = offsets.into_iter().collect();
    // offsets_rev.sort_by_key(|(_, g)| g.values().map(|v| v.len()).sum::<usize>());
    // offsets_rev.reverse();
    // assert!(offsets_rev.len() != 0, "offsets_rev should not be empty");

    // let mut candidate: HashMap<Tone, Vec<usize>> = offsets_rev[0].1.clone();
    // let candidate_len: usize = candidate.iter().map(|(_, g)| g.len()).sum();
    // assert!(candidate_len >= 2, "candidate should have at least 2 tones");

    // for (_offset, tones) in &offsets_rev {
    //     // Find the intersection
    //     let intersection: HashMap<Tone, Vec<usize>> = tones
    //         .iter()
    //         .filter_map(|(tone, indices)| {
    //             candidate.get(tone).map(|candidate_indices| {
    //                 // Find the intersection of the two Vecs
    //                 let common: Vec<usize> = indices
    //                     .iter()
    //                     .filter(|i| candidate_indices.contains(i))
    //                     .copied()
    //                     .collect();
    //                 (tone.clone(), common)
    //             })
    //         })
    //         .collect();

    //     let intersection_len: usize = intersection.values().map(|v| v.len()).sum();
    //     if intersection_len >= 2 {
    //         candidate = intersection;
    //     }
    // }

    // song.notes = reassign_layers(final_maps);
    // song.header.is_loop = true;
    // song.save_nbs("fixtures/analyzed.nbs").unwrap();
}

// track
//
// ============================================================================

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
