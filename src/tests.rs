use super::*;
use counter::Counter;
use mcdata::{BlockState, util::BlockPos};
use ordered_float::OrderedFloat;
use rustmatica::Region;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::iter::repeat;
use std::ops::Range;
use std::{result, vec};
type Multiset<T> = counter::Counter<T>;

// TEST: temporary test case
/// Pre-defined patterns for note block arrangement.
pub const PATTERNS: &[&[Index]] = &[
    // &[0, 64, 128, 192, 32, 96, 160, 224],
    // &[0, 64, 128, 192],
    // &[0, 128],
    // &[0, 1, 16, 17],
    // &[0, 1, 4, 5],
    // &[0, 1, 4, 5, 16, 17, 20, 21],
    // &[0, 32],
    // &[0, 16, 32, 48, 64, 80, 96, 112],
    // &[0, 16],
    // &[0, 8, 16, 24, 32, 40, 48, 56, 64, 72, 80, 88],
    // &[0, 16, 32, 48, 64, 80],
    // &[0, 16],
    // &[0, 64, 32, 96, 16, 48, 80, 112, 24, 40, 56, 72, 88, 104, 120],
    // &[0, 64, 32, 96, 16, 48, 80, 112],
    // &[0, 64, 32, 96],
    // &[0, 64],
    //
    // &[0, 8, 16, 24, 32, 40, 48, 56, 64, 72, 80, 88, 96, 104],
    // &[0, 16, 32, 48, 64, 80, 96],
    // &[0, 1, 8, 9, 16, 17, 24, 25],
    // &[0, 64, 32, 96],
    // &[0, 64],
    &[0, 4, 8, 12, 16, 20, 24, 28],
    &[0, 8, 16, 24],
    &[0, 16],
    //

    // &[
    //     16 * 0,
    //     16 * 1,
    //     16 * 2,
    //     16 * 3,
    //     16 * 4,
    //     16 * 5,
    //     16 * 6,
    //     16 * 7,
    // ],
    // &[32 * 0, 32 * 1, 32 * 2, 32 * 3],
    // &[64 * 0, 64 * 1],
    &[0],
];

#[test]
fn test_scale_ticks() {
    let mut song = Song::open_nbs("fixtures/source.nbs").unwrap();

    // Scale each note's tick by 3
    let scaled_notes: BTreeMap<Position, Note> = song
        .notes
        .into_iter()
        .map(|(pos, note)| {
            let new_pos = Position::new(pos.tick() * 3, pos.layer);
            (new_pos, note)
        })
        .collect();

    song.notes = scaled_notes;

    // Also update song length metadata if present
    song.header.song_length = song.header.song_length * 3;

    song.save_nbs("fixtures/scaled_by_3.nbs").unwrap();
}

#[test]
fn test_v6_to_v5_conversion() {
    let mut song_v6 = Song::open_nbs("fixtures/source.nbs").unwrap();
    assert_eq!(song_v6.header.version, Version(6));

    song_v6.header.version = Version(5);
    song_v6.save_nbs("fixtures/out_v5.nbs").unwrap();
}

// cargo test test_generation_litematic && cargo test test_pattern_matching && cargo test analyze_tones

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
fn test_sectional_matching() {
    let mut song = Song::open_nbs("fixtures/source.nbs").unwrap();
    let notes = song.notes;

    let song_length: Index = 1024;
    let coarse: Index = 4;

    let global_patterns: &[&[Index]] = &[
        // &[0, 8 * 1, 8 * 2, 8 * 3, 8 * 4, 8 * 5, 8 * 6, 8 * 7],
        // &[0, 8 * 1, 8 * 2, 8 * 3],
        // &[0, 12, 24, 36],
        &[0, 32, 192, 224, 256, 288, 320, 352, 384, 416, 448, 480],
        &[0, 32, 64, 96, 192, 256, 288, 320, 352, 384, 416, 448, 480],
        &[0, 192, 256, 288, 320, 352, 384, 416],
        &[0, 32, 64, 96],
        &[0, 64],
        &[0, 32],
        &[0],
        // &[0, 16, 32, 48, 64, 80, 96, 112],
        // &[0, 8, 16, 24, 32, 40, 48, 56],
        // &[0, 16, 32, 48],
        // &[0, 32],
        // &[0],
    ];
    // let sectional_patterns: &[&[Index]] = &[&[0, 8, 16, 24], &[0, 16], &[0]];
    // let sections: &[Range<Index>] = &[0..32, 32..64, 64..96, 96..128];
    let sectional_patterns: &[&[Index]] = &[];
    let sections: &[Range<Index>] = &[];

    let mut all_clusters: Vec<BTreeMap<Position, Note>> = vec![];

    // 第一步：全局匹配
    let mut remaining = notes.clone();
    for &pattern in global_patterns {
        let (matched, unmatched) =
            remaining.matches_by(pattern, song_length, |a, b| a.tone() == b.tone());
        all_clusters.push(matched);
        remaining = unmatched;
    }

    // 第二步：未匹配上的进入章节匹配
    for section_range in sections {
        // 提取当前章节的未匹配音符，保留原始 tick 位置（不动原点）
        let section_notes: BTreeMap<Position, Note> = remaining
            .clone()
            .into_iter()
            .filter(|(p, _)| section_range.contains(&p.tick()))
            .collect();

        if section_notes.is_empty() {
            continue;
        }

        let mut remaining_in_section = section_notes;
        for &pattern in sectional_patterns {
            let (matched, unmatched) =
                remaining_in_section.matches_by(pattern, song_length, |a, b| a.tone() == b.tone());
            all_clusters.push(matched);
            remaining_in_section = unmatched;
        }
    }

    // 输出 nbs
    song.notes = reassign_layers(all_clusters.clone());
    song.header.is_loop = true;
    song.save_nbs("fixtures/out_sectional.nbs").unwrap();

    // 输出投影 (litematic)
    let tracks: Vec<Track> = all_clusters
        .into_iter()
        .map(|cluster| Track::new(cluster, coarse))
        .collect();

    let mut region: Region<GenericBlockState> = Region::new(
        "Planet",
        BlockPos::new(0, 0, 0),
        BlockPos::new(
            (tracks.len() * 3) as _,
            4,
            (tracks.iter().map(Track::len).max().unwrap_or(0)) as _,
        ),
    );

    for (track_idx, track) in tracks.iter().enumerate() {
        let iter = TrackIterator::new(track);
        for (block_pos, block_state) in iter {
            let x_offset = track_idx as i32 * 3;
            let world_pos = BlockPos::new(x_offset + block_pos.x, block_pos.y, block_pos.z);
            region.set_block(world_pos, block_state);
        }
    }

    let planet = region.as_litematic("Sectional from source.nbs", "Planet");
    planet
        .write_file("fixtures/generated_sectional.litematic")
        .unwrap();
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
fn reassign_layers_generic<I, J>(slices: I) -> BTreeMap<Position, Note>
where
    I: IntoIterator<Item = J>,
    J: IntoIterator<Item = (Index, Note)>,
{
    let mut base_layer: Index = Default::default();
    let mut result: BTreeMap<Position, Note> = Default::default();

    for notes in slices {
        // 收集并排序，确保 tick 顺序正确
        let mut notes: Vec<(Index, Note)> = notes.into_iter().collect();
        notes.sort_unstable_by_key(|(tick, _)| *tick);
        let mut notes = notes
            .into_iter()
            .map(|(tick, note)| (Position::new(tick, 0), note))
            .peekable();

        let mut current_layer: Index = Default::default();
        let mut max_layer: Index = Default::default();

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

// 按照列表重新分配层级
fn reassign_layers(slices: Vec<BTreeMap<Position, Note>>) -> BTreeMap<Position, Note> {
    reassign_layers_generic(
        slices
            .into_iter()
            .map(|map| map.into_iter().map(|(pos, note)| (pos.tick(), note))),
    )
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

    // Plane-form notes multiset [note: (x: tick, y: tone), ...]
    let mut notes_multiset: Vec<Point> = song
        .notes
        .into_iter()
        .map(|(p, n)| (p.tick(), n.tone()))
        .collect();
    notes_multiset.sort_unstable();

    // Left point of translation {offset: [note_point, ...], ...}
    let offsets: HashMap<Index, Vec<Point>> = notes_multiset
        .iter()
        .enumerate()
        .flat_map(|(i, r)| notes_multiset[..i].iter().map(move |l| (l, r)))
        .fold(Default::default(), |mut acc, ((tl, nl), (tr, nr))| {
            let offset = match nr == nl {
                true => (tr - tl).min(song_length - (tr - tl)),
                false => return acc,
            };
            acc.entry(offset).or_default().push((*tl, *nl));
            acc
        });

    let mut offset_counts: Vec<(usize, Index)> = offsets
        .into_iter()
        .filter(|(_, v)| v.len() > 1)
        .map(|(k, v)| (v.len(), k))
        .collect();
    offset_counts.sort_unstable_by(|a, b| b.cmp(&a));

    // TEST: This theory is unstable

    let mut subset: Counter<Point> = Default::default();
    let mut pattern: HashSet<Index> = From::from([0]);

    for &(count, offset) in &offset_counts {
        // if count < subset.len() {
        //     continue;
        // }

        let pat = {
            let mut pat = pattern.clone();
            pat.insert(offset);
            pat
        };
        let sub = satisfy_constraints(
            &mut notes_multiset.iter().copied().collect(),
            &pat,
            song_length,
        );
        if sub.len() >= subset.len() {
            subset = sub;
            pattern = pat;

            print!(
                "offset {}: count {}, pattern: {:?}, subset:",
                offset, count, pattern
            );
            for ((tick, _), cnt) in &subset {
                print!(" ({}: {}),", tick, cnt);
            }
            println!("");
        }
    }

    // Collect matched subset and remaining notes into slices
    let mut subset_counts: HashMap<Point, usize> = subset.iter().map(|(&p, &c)| (p, c)).collect();
    let mut matched_notes: Vec<(Index, Note)> = Vec::new();
    let mut remaining_notes: Vec<(Index, Note)> = Vec::new();

    for &(tick, tone) in &notes_multiset {
        let note: Note = tone.into();
        if let Some(count) = subset_counts.get_mut(&(tick, tone)) {
            if *count > 0 {
                matched_notes.push((tick, note));
                *count -= 1;
                continue;
            }
        }
        remaining_notes.push((tick, note));
    }

    song.notes = reassign_layers_generic(vec![matched_notes, remaining_notes]);
    song.header.is_loop = true;
    song.save_nbs("fixtures/transposition.nbs").unwrap();
}

// A note point in the (tick, tone) plane of the score
type Point = (Index, Tone);

/// Approximate Minkowski sum decomposition via conflict resolution for noisy data.
///
/// Find all possible positions for the `pattern`,
/// then iteratively select the group with the least conflict.
///
/// Suppose multiset = [0, 1, 2, 3, 3, 5, 6], pattern = [0, 1, 3], Then:
///
/// | 0^1 | 1^1 | 2^1 | 3^2 | 5^1 | 6^1 |
/// |-----|-----|-----|-----|-----|-----|
/// | A0  | A1  |     | A3  |     |     |
/// |     | B0  | B1  |     | B3  |     |
/// |     |     | C0  | C1  |     | C3  |
///
/// Group A overlap degree: 4 (1/1 + 2/1 + 2/2)
/// Group B overlap degree: 5 (2/1 + 2/1 + 1/1)
/// Group C overlap degree: 4 (2/1 + 2/2 + 1/1)
pub fn satisfy_constraints(
    multiset: &mut Counter<Point>,
    pattern: &HashSet<Index>,
    song_length: Index,
) -> Counter<Point> {
    debug_assert!(pattern.len() > 1);
    debug_assert!(pattern.get(&0).is_some());
    // if cfg!(debug_assertions) {
    //     print!("multiset:");
    //     for (point, count) in multiset.iter() {
    //         let (tick, (inst, key)) = point;
    //         print!(" ({}: {}, {}, x{})", tick, inst, key, count);
    //     }
    //     println!();
    //     println!("pattern: {:?}", pattern);
    //     println!("song_length: {:?}", song_length);
    //     println!();
    // }

    // Any positions satisfying the matching pattern
    let find_groups = |multiset: &Counter<Point>| -> Option<Vec<BTreeSet<Point>>> {
        let groups: Vec<BTreeSet<Point>> = multiset
            .keys()
            .filter_map(|&(anchor, tone)| {
                let map_pat = |pat: &Index| ((anchor + pat) % song_length, tone);
                let has_stock = |pp: &Point| multiset.contains_key(pp);
                let group: BTreeSet<Point> = pattern.iter().map(map_pat).collect();
                group.iter().all(has_stock).then_some(group)
            })
            .collect();
        (!groups.is_empty()).then_some(groups)
    };

    // Matched multiset
    let mut result: Counter<Point> = Default::default();

    while let Some(groups) = find_groups(&multiset) {
        // Count how many groups cover each point (multiplicity)
        let multiplicity: Counter<Point> = groups.iter().flat_map(|g| g).copied().collect();

        // Least conflict; tie-break by smallest point
        let chosen_group = groups
            .into_iter()
            .min_by_key(|group| {
                let score: f32 = group
                    .iter()
                    .map(|&pp| multiplicity[&pp] as f32 / multiset[&pp] as f32)
                    .sum();
                (OrderedFloat(score), group.first().copied())
            })
            .unwrap();

        // Move the chosen group to result
        multiset.subtract(chosen_group.iter().copied());
        result.extend(chosen_group);
    }
    result
}

//
//
// ============================================================================

#[test]
// #[deprecated]
pub fn test_deconvolve_d1() {
    // TODO: 优化空间巨大，但先验证理论模型
    //       开放性问题，评价模型尚不完善
    // #[cfg(not(debug_assertions))]
    // compile_error!("unimplemented path in release");

    /// find best pattern arrangement via backtracking
    fn deconvolve(points_mset: &Vec<Point>, loop_len: Index) -> Vec<Point> {
        let mut result: Vec<Point> = Default::default();
        let mut candidate: Vec<Point>;
        // stack
        let mut task = vec![points_mset.iter()];
        let mut pattern: Vec<Point> = vec![];
        let mut base_size: Vec<usize> = vec![0];

        while !task.is_empty() {
            // backtrack
            let Some(p) = task.last_mut().unwrap().next() else {
                task.pop();
                pattern.pop();
                base_size.pop();
                continue;
            };
            // matchs
            pattern.push(*p);
            candidate = place_pattern(points_mset, &pattern, loop_len);

            if candidate.len() >= *base_size.last().unwrap() {
                // recurse
                task.push(task.last().unwrap().clone());
                base_size.push(candidate.len());
                // elected
                if candidate.len() > result.len() {
                    result = candidate;
                } // TODO: tie-break if equal
            } else {
                pattern.pop();
            }
        }
        result
    }

    /// sequential matching. sensitive to input, prone to local optima
    fn place_pattern(points_mset: &[Point], pattern: &[Point], loop_len: Index) -> Vec<Point> {
        let mut points: Counter<Point> = points_mset.iter().copied().collect();
        let pattern: Counter<Point> = pattern.iter().copied().collect();
        let mut result = Vec::with_capacity(points.len());

        for i in 0..loop_len {
            let offset_pattern: Counter<Point> = pattern
                .iter()
                .map(|(&(t, n), &c)| (((t + i) % loop_len, n), c))
                .collect();

            if offset_pattern
                .iter()
                .all(|(p, &c)| points.get(p).copied().unwrap_or(0) >= c)
            {
                for (p, &c) in &offset_pattern {
                    *points.get_mut(p).unwrap() -= c;
                    result.extend(repeat(*p).take(c));
                }
            }
        }
        result
    }

    //

    let mut song = Song::open_nbs("fixtures/source.nbs").unwrap();

    // params
    let song_length: Index = song.notes.iter().map(|(p, _)| p.tick()).max().unwrap() + 1;

    // 构建点集 multiset
    let points_mset: Vec<Point> = song
        .notes
        .iter()
        .map(|(pos, note)| (pos.tick(), (note.instrument, note.key)))
        .collect();

    // 找到最大重复模式
    let pattern = deconvolve(&points_mset, song_length);

    // 将音符分为模式匹配部分和剩余部分
    let mut pattern_counter: Counter<Point> = pattern.iter().copied().collect();
    let mut matched: Vec<(Index, Note)> = Vec::new();
    let mut remaining: Vec<(Index, Note)> = Vec::new();

    for (pos, note) in &song.notes {
        let p = (pos.tick(), (note.instrument, note.key));
        if pattern_counter.get(&p).copied().unwrap_or(0) > 0 {
            *pattern_counter.get_mut(&p).unwrap() -= 1;
            matched.push((pos.tick(), note.clone()));
        } else {
            remaining.push((pos.tick(), note.clone()));
        }
    }

    song.notes = reassign_layers_generic(vec![matched, remaining]);
    song.header.is_loop = true;
    song.save_nbs("fixtures/deconvolve.nbs").unwrap();
}

#[test]
pub fn test_deconvolve() {
    fn deconvolve(points_mset: &[Point], loop_len: Index) -> Vec<Point> {
        let points: Counter<Point> = points_mset.iter().copied().collect();
        let mut local_points: HashMap<Point, Counter<Index>> = points
            .keys()
            .map(|&(tick, note)| {
                let offset_by_tick = |(&(ti, _), &c)| ((ti + loop_len - tick) % loop_len, c);
                ((tick, note), points.iter().map(offset_by_tick).collect())
            })
            .collect();
        // let mut local_points: HashMap<Point, HashSet<Index>> = points.keys()
        //     .map(|&(tick, note)| {
        //         let offset_by_tick = |&(ti, _)| (ti + loop_len - tick) % loop_len;
        //         ((tick, note), points.keys().map(offset_by_tick).collect())
        //     }).collect();

        // {N} -> {{K}} * {T} = {{M}} - residual
        let mut note_palette: HashSet<Tone> = Default::default();
        let mut time_pattern: HashSet<Index> = Default::default();
        let mut conv_kernel: Counter<Index> = Default::default();

        // TODO: 1. 建立初始值
        //       2. 迭代贪婪的匹配，消耗 loval_points 对 conv_kernel 做不缩小匹配
        //       3. 重建为整体返回

        while !local_points.is_empty() {
            //
        }

        todo!()
    }

    fn seed_best<'a, I>(local_points: I) -> Counter<Index>
    where
        I: IntoIterator<Item = (&'a Point, &'a Counter<Index>)>,
    {
        let local_points: Vec<(&Point, &Counter<Index>)> = local_points.into_iter().collect();
        // local_points.sort_unstable_by_key(|(p, _)| *p);
        let mut best_size: usize = Default::default();
        let mut best: Counter<Index> = Default::default();

        // TODO: 先比较最多的两个局部坐标系的交集作为迭代初始值。
        // TODO: 一个音在时间上的图案，不随时间变化而变化，而只是位移。
        //       同种音和不同音的匹配行为应该有区分。
        for ((&(_tkl, _ntl), cntl), (&(_tkr, _ntr), cntr)) in local_points
            .iter()
            .enumerate()
            .flat_map(|(i, r)| local_points[..i].iter().map(move |l| (*l, *r)))
        {
            // if ntl == ntr {}

            let candidate: Counter<Index> = cntl
                .iter()
                .filter_map(|(&t, &cnt)| {
                    cntr.get(&t)
                        .map(|&cr| cnt.min(cr))
                        .filter(|&c| c > 0)
                        .map(|c| (t, c))
                })
                .collect();

            let candidate_size = candidate.values().sum();
            if candidate_size > best_size {
                best_size = candidate_size;
                best = candidate;
            }
        }
        best
    }

    /// sequential matching. sensitive to input, prone to local optima
    fn place_pattern(points_mset: &[Point], pattern: &[Point], loop_len: Index) -> Vec<Point> {
        let mut points: Counter<Point> = points_mset.iter().copied().collect();
        let pattern: Counter<Point> = pattern.iter().copied().collect();
        let mut result = Vec::with_capacity(points.len());

        for i in 0..loop_len {
            let offset_pattern: Counter<Point> = pattern
                .iter()
                .map(|(&(t, n), &c)| (((t + i) % loop_len, n), c))
                .collect();

            if offset_pattern
                .iter()
                .all(|(p, &c)| points.get(p).copied().unwrap_or(0) >= c)
            {
                for (p, &c) in &offset_pattern {
                    *points.get_mut(p).unwrap() -= c;
                    result.extend(repeat(*p).take(c));
                }
            }
        }
        result
    }

    //

    let mut song = Song::open_nbs("fixtures/source.nbs").unwrap();

    // params
    let song_length: Index = song.notes.iter().map(|(p, _)| p.tick()).max().unwrap() + 1;

    // 构建点集 multiset
    let points_mset: Vec<Point> = song
        .notes
        .iter()
        .map(|(pos, note)| (pos.tick(), (note.instrument, note.key)))
        .collect();

    // 找到最大重复模式
    let pattern = deconvolve(&points_mset, song_length);

    // 将音符分为模式匹配部分和剩余部分
    let mut pattern_counter: Counter<Point> = pattern.iter().copied().collect();
    let mut matched: Vec<(Index, Note)> = Vec::new();
    let mut remaining: Vec<(Index, Note)> = Vec::new();

    for (pos, note) in &song.notes {
        let p = (pos.tick(), (note.instrument, note.key));
        if pattern_counter.get(&p).copied().unwrap_or(0) > 0 {
            *pattern_counter.get_mut(&p).unwrap() -= 1;
            matched.push((pos.tick(), note.clone()));
        } else {
            remaining.push((pos.tick(), note.clone()));
        }
    }

    song.notes = reassign_layers_generic(vec![matched, remaining]);
    song.header.is_loop = true;
    song.save_nbs("fixtures/deconvolve.nbs").unwrap();
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
