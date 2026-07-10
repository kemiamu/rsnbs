use crate::layout::CompactLayout;
use crate::schematic::SchematicBuilder;
use crate::util::MatchedGroups;
use crate::{GameTick, Index, Note, Notes, Position, Song, Tick, Tone, Version};
use counter::Counter;
use ordered_float::OrderedFloat;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::iter::repeat;
use std::num::NonZero;
use std::ops::Range;

type Multiset<T> = BTreeMap<T, NonZero<usize>>;

// A note point in the (tick, tone) plane of the score
type Point = (Tick, Tone);

#[test]
fn test_scale_ticks() {
    let mut song = Song::open_nbs("fixtures/source.nbs").unwrap();

    const NUM: Index = 3;
    const DEN: Index = 1;

    // Scale each note's tick by numerator/denominator, multiply first
    let scaled_notes: Notes = song
        .notes
        .into_iter()
        .map(|(pos, note)| {
            let new_tick = pos.tick() * NUM / DEN;
            let new_pos = Position::new(new_tick, pos.layer());
            (new_pos, note)
        })
        .collect();

    song.notes = scaled_notes;

    // Also update song length metadata if present
    song.header.song_length = song.header.song_length * NUM / DEN;

    song.save_nbs("fixtures/scaled.nbs").unwrap();
}

#[test]
fn test_v6_to_v5_conversion() {
    let mut song_v6 = Song::open_nbs("fixtures/source.nbs").unwrap();
    assert_eq!(song_v6.header.version, Version::new(6).unwrap());

    song_v6.header.version = Version::new(5).unwrap();
    song_v6.save_nbs("fixtures/out_v5.nbs").unwrap();
}

// cargo test test_sectional_matching && cargo test analyze_tones

#[test]
#[allow(deprecated)]
fn test_sectional_matching() {
    let mut song = Song::open_nbs("fixtures/source.nbs").unwrap();
    let notes = song.notes.clone();

    // let song_length: Index = 128;
    let song_length: Index = notes.iter().map(|(pos, _)| pos.tick()).max().unwrap_or(0) + 1;
    let min_notes: usize = 0;
    let coarse: GameTick = 0;
    let wrap_length: usize = 0;

    // 匹配+回退包装：匹配音符数不足时回退所有匹配
    let try_match = |notes: Notes, pattern: &[Index]| -> (MatchedGroups, Notes) {
        let saved = notes.clone();
        let (matched, unmatched) =
            notes.group_match(pattern, song_length, |a, b| a.tone() == b.tone());
        if matched.matched_len() >= min_notes || pattern.len() == 1 {
            (matched, unmatched)
        } else {
            (MatchedGroups::empty(), saved)
        }
    };

    // let global_patterns: &[&[Index]] = &[
    //     &[0, 32, 192, 224, 256, 288, 320, 352, 384, 416, 448, 480],
    //     &[0, 32, 64, 96, 192, 256, 288, 320, 352, 384, 416, 448, 480],
    //     &[0, 192, 256, 288, 320, 352, 384, 416],
    // ];
    // let sectional_patterns: &[&[Index]] = &[&[0, 16, 32, 48], &[0, 16], &[0]];
    // let sections: &[Range<Index>] = &[0..256, 256..512];
    let global_patterns: &[&[Index]] = &[
        // &[0, 16, 16 * 2, 16 * 3, 16 * 4, 16 * 5, 16 * 6, 16 * 7],
        // &[0, 32, 32 * 2, 32 * 3, 4, 32 + 4, 32 * 2 + 4, 32 * 3 + 4],
        // &[0, 32, 32 * 2, 32 * 3],
        // &[0, 64, 4, 64 + 4],
        // &[0, 64],
        &[0], // any
    ];
    let sectional_patterns: &[&[Index]] = &[];
    let sections: &[Range<Index>] = &[];

    let mut all_matched: Vec<MatchedGroups> = vec![];

    // 第一步：全局匹配
    let mut remaining = notes.clone();
    for &pattern in global_patterns {
        let (matched, unmatched) = try_match(remaining, pattern);
        all_matched.push(matched);
        remaining = unmatched;
    }

    // 第二步：未匹配上的进入章节匹配
    for section_range in sections {
        let section_notes: Notes = remaining
            .clone()
            .into_iter()
            .filter(|(p, _)| section_range.contains(&p.tick()))
            .collect();

        if section_notes.is_empty() {
            continue;
        }

        let mut remaining_in_section = section_notes;
        for &pattern in sectional_patterns {
            let (matched, unmatched) = try_match(remaining_in_section, pattern);
            all_matched.push(matched);
            remaining_in_section = unmatched;
        }
    }

    // 输出 nbs
    song.notes = Notes::reassign_layers(all_matched.iter().map(|mg| {
        mg.groups()
            .iter()
            .flat_map(|g| g.iter().map(|(p, n)| (p.tick(), n.clone())))
    }));
    song.header.is_loop = true;
    song.save_nbs("fixtures/out_sectional.nbs").unwrap();

    // 输出 litematic
    let projection_clusters: Vec<Notes> = all_matched.iter().map(|mg| mg.templates()).collect();

    let tempo = song.header.tempo;
    let scale = (20.0 / tempo).round() as u32;

    let tracks = projection_clusters.into_iter().map(|cluster| {
        let mut notes: BTreeMap<Tick, Vec<Note>> = BTreeMap::new();
        for (pos, note) in cluster {
            let tick = if scale > 1 {
                pos.tick() * scale
            } else {
                pos.tick()
            };
            notes.entry(tick).or_default().push(note);
        }
        (notes, NonZero::new(coarse))
    });
    let layout = CompactLayout::new(tracks, NonZero::new(wrap_length), 0);
    let litematic = SchematicBuilder(layout).build("Sectional from source.nbs", "Planet");
    litematic
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
    let slices: Vec<Notes> = by_tone
        .into_values()
        .map(|v| v.into_iter().collect())
        .collect();

    song.notes = Notes::reassign_layers(
        slices
            .into_iter()
            .map(|m| m.into_iter().map(|(p, n)| (p.tick(), n))),
    );
    song.header.is_loop = true;
    song.save_nbs("fixtures/analyzed.nbs").unwrap();
}

//
//
// ============================================================================

#[test]
fn test_deconvolve_m1() {
    let song = Song::open_nbs("fixtures/source.nbs").unwrap();

    let loop_length = song.len();
    let half_loop = loop_length / 2;

    let points: Vec<Point> = song
        .notes
        .iter()
        .map(|(pos, note)| (pos.tick(), note.tone()))
        .collect();

    let pairs = points
        .iter()
        .enumerate()
        .flat_map(|(i, r)| points[..i].iter().map(move |l| (l, r)));

    // left point of translation `{offset: {&tone: [&tick]}}`
    let mut offsets: HashMap<Tick, HashMap<&Tone, Multiset<&Tick>>>;

    offsets = pairs.fold(Default::default(), |mut acc, pair| {
        let (left @ &(tl, nl), right @ &(tr, nr)) = pair;
        let offset = match nr == nl {
            true => tr - tl,
            false => return acc,
        };
        let ((tick, tone), offset) = match offset <= half_loop {
            true => (left, offset),
            false => (right, loop_length - offset),
        };
        acc.entry(offset)
            .or_default()
            .entry(tone)
            .or_default()
            .entry(tick)
            .and_modify(|c| *c = c.saturating_add(1))
            .or_insert(NonZero::new(1).unwrap());
        acc
    });

    // 过滤自重叠
    for (offset, multiset) in offsets
        .iter_mut()
        .flat_map(|(&offset, tones)| tones.values_mut().map(move |m| (offset, m)))
    {
        let old = std::mem::take(multiset);
        let mut mset: Multiset<&Tick> = Default::default();
        for (&tick, count) in &old {
            let pred = (loop_length + tick - offset) % loop_length;
            let base = mset.get(&pred).map(|&c| c.get()).unwrap_or_default();
            let kept = count.get().saturating_sub(base);
            let _ = NonZero::new(kept).map(|k| mset.insert(tick, k));
        }
        *multiset = mset;
    }

    todo!()
}

//
//
// ============================================================================

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

    song.notes = Notes::reassign_layers(vec![matched_notes, remaining_notes]);
    song.header.is_loop = true;
    song.save_nbs("fixtures/transposition.nbs").unwrap();
}

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

    song.notes = Notes::reassign_layers(vec![matched, remaining]);
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

    song.notes = Notes::reassign_layers(vec![matched, remaining]);
    song.header.is_loop = true;
    song.save_nbs("fixtures/deconvolve.nbs").unwrap();
}
