use clap::{Parser, Subcommand};
use rsnbs::schematic::SchematicBuilder;
use rsnbs::util::NotesExt;
use rsnbs::{Index, Note, Position, Song, Tone};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ops::Range;

#[derive(Parser)]
#[command(name = "rsnbs", about = "NBS song to Minecraft litematic converter")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Pattern matching and sectional matching
    Matching {
        /// Input NBS file
        input: String,
        /// Output directory
        #[arg(short, long, default_value = "out")]
        output: String,
    },
    /// Group notes by tone and reassign layers
    Analyze {
        /// Input NBS file
        input: String,
        /// Output directory
        #[arg(short, long, default_value = "out")]
        output: String,
    },
    /// Analyze transposition equivalence (same-tone point pairs)
    AnalyzeOffset {
        /// Input NBS file
        input: String,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Command::Matching { input, output } => matching(&input, &output),
        Command::Analyze { input, output } => analyze(&input, &output),
        Command::AnalyzeOffset { input } => analyze_offset(&input),
    }
}

// cargo run -- matching fixtures/source.nbs && cargo run -- analyze fixtures/source.nbs

fn matching(input: &str, output: &str) {
    std::fs::create_dir_all(output).unwrap();
    let nbs_path = std::path::Path::new(output).join("matched.nbs");
    let litematic_path = std::path::Path::new(output).join("generated_matched.litematic");
    let song = Song::open_nbs(input).unwrap();
    let notes = song.notes.clone();

    // hardcoded parameters
    let song_length: Index = 512;
    let min_notes: usize = 0;
    let coarse: Index = 0;
    let wrap_length: usize = 16;

    // matching + 回退包装
    let try_match = |notes: BTreeMap<Position, Note>,
                     pattern: &[Index]|
     -> (BTreeMap<Position, Note>, BTreeMap<Position, Note>) {
        let saved = notes.clone();
        let (matched, unmatched) =
            notes.matches_by(pattern, song_length, |a, b| a.tone() == b.tone());
        if matched.len() >= min_notes || pattern.len() == 1 {
            (matched, unmatched)
        } else {
            (BTreeMap::new(), saved)
        }
    };

    let global_patterns: &[&[Index]] = &[
        &[
            0, 16, 32, 48, 192, 208, 224, 240, 256, 272, 288, 304, 320, 336, 352, 368, 384, 400,
            416, 432, 448, 464, 480, 496,
        ],
        &[
            0, 16, 32, 48, 64, 80, 96, 112, 192, 208, 256, 272, 288, 304, 320, 336, 352, 368, 384,
            400, 416, 432, 448, 464, 480, 496,
        ],
        &[
            0, 16, 32, 192, 208, 256, 272, 288, 304, 320, 336, 352, 368, 384, 400, 416, 432,
        ],
        &[0, 64, 128, 64 * 3],
        &[0, 16, 32, 48],
        &[0, 16],
        &[0, 64],
        &[0],
    ];
    // let sectional_patterns: &[&[Index]] = &[&[0, 16, 32, 48], &[0, 16], &[0]];
    // let sections: &[Range<Index>] = &[0..256, 256..512];
    let sectional_patterns: &[&[Index]] = &[];
    let sections: &[Range<Index>] = &[];

    let mut all_clusters: Vec<BTreeMap<Position, Note>> = vec![];

    // 全局匹配
    let mut remaining = notes.clone();
    for &pattern in global_patterns {
        let (matched, unmatched) = try_match(remaining, pattern);
        all_clusters.push(matched);
        remaining = unmatched;
    }

    // 章节匹配
    for section_range in sections {
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
            let (matched, unmatched) = try_match(remaining_in_section, pattern);
            all_clusters.push(matched);
            remaining_in_section = unmatched;
        }
    }

    // 保存 NBS（重新分配层）
    let mut matched_song = song;
    matched_song.notes = <BTreeMap<Position, Note> as NotesExt>::reassign_layers(
        all_clusters
            .iter()
            .map(|c| c.iter().map(|(p, n)| (p.tick(), n.clone()))),
    );

    matched_song.save_nbs(&nbs_path).unwrap();

    // 过滤 projection
    let projection_clusters: Vec<BTreeMap<Position, Note>> = all_clusters
        .iter()
        .zip(global_patterns.iter().cycle())
        .map(|(cluster, pattern)| {
            let ticks: BTreeSet<Index> = cluster.keys().map(|p| p.tick()).collect();
            cluster
                .iter()
                .filter(|(pos, _)| {
                    let base = pos.tick();
                    pattern
                        .iter()
                        .skip(1)
                        .all(|offset| ticks.contains(&((base + offset) % song_length)))
                })
                .map(|(pos, note)| (*pos, note.clone()))
                .collect()
        })
        .collect();

    // 用 SchematicBuilder 构建 litematic
    let mut builder = SchematicBuilder::new().with_wrap_length(wrap_length);

    for cluster in projection_clusters {
        builder.add_track(
            cluster.into_iter().map(|(pos, note)| (pos.tick(), note)),
            coarse,
        );
    }

    let litematic = builder.build("from source.nbs", "rsnbs");
    litematic.write_file(&litematic_path).unwrap();

    println!("Done: {}, {}", nbs_path.display(), litematic_path.display());
}

fn analyze(input: &str, output: &str) {
    let song = Song::open_nbs(input).unwrap();

    let mut by_tone: BTreeMap<Tone, Vec<(Position, Note)>> = BTreeMap::new();
    for (pos, note) in song.notes.clone() {
        by_tone.entry(note.tone()).or_default().push((pos, note));
    }
    let slices: Vec<BTreeMap<Position, Note>> = by_tone
        .into_values()
        .map(|v| v.into_iter().collect())
        .collect();

    let nbs_path = std::path::Path::new(output).join("analyzed.nbs");
    std::fs::create_dir_all(output).unwrap();
    let mut analyzed = song;
    analyzed.notes = <BTreeMap<Position, Note> as NotesExt>::reassign_layers(
        slices
            .iter()
            .map(|c| c.iter().map(|(p, n)| (p.tick(), n.clone()))),
    );
    analyzed.save_nbs(&nbs_path).unwrap();
    println!("Done: {}", nbs_path.display());
}

fn analyze_offset(input: &str) {
    let song = Song::open_nbs(input).unwrap();
    let song_length: Index = song.notes.iter().map(|(p, _)| p.tick()).max().unwrap() + 1;

    // 音符视为点 (tick, tone)
    let mut notes: Vec<(Index, Tone)> = song
        .notes
        .into_iter()
        .map(|(p, n)| (p.tick(), n.tone()))
        .collect();
    notes.sort_unstable();

    // 按音高分组，每组按时间升序
    let by_tone: BTreeMap<Tone, Vec<Index>> =
        notes
            .iter()
            .fold(Default::default(), |mut acc, &(t, tone)| {
                acc.entry(tone).or_default().push(t);
                acc
            });

    // ===== 2 点：原逻辑 =====
    {
        let offsets: HashMap<Index, Vec<(Index, Tone)>> = notes
            .iter()
            .enumerate()
            .flat_map(|(i, r)| notes[..i].iter().map(move |l| (l, r)))
            .fold(Default::default(), |mut acc, ((tl, nl), (tr, nr))| {
                if nr != nl {
                    return acc;
                }
                let offset = (tr - tl).min(song_length - (tr - tl));
                acc.entry(offset).or_default().push((*tl, *nl));
                acc
            });

        let mut counts: Vec<(usize, Index)> = offsets
            .into_iter()
            .filter(|(_, v)| v.len() > 1)
            .map(|(k, v)| (v.len(), k))
            .collect();
        counts.sort_unstable_by(|a, b| b.cmp(&a));
        println!("=== 2-point (offset: count) top50 ===");
        for (count, offset) in &counts[..50.min(counts.len())] {
            println!("  {offset:>4}: {count}");
        }
    }

    // ===== 3 点：距离向量 (d1, d2) =====
    {
        let mut triples: HashMap<(Index, Index), Vec<(Index, Tone)>> = HashMap::new();
        for (&tone, ticks) in &by_tone {
            for i in 0..ticks.len() {
                for j in i + 1..ticks.len() {
                    for k in j + 1..ticks.len() {
                        let d1 = ticks[j] - ticks[i];
                        let d2 = ticks[k] - ticks[j];
                        triples.entry((d1, d2)).or_default().push((ticks[i], tone));
                    }
                }
            }
        }
        let mut triple_counts: Vec<(usize, (Index, Index))> = triples
            .into_iter()
            .filter(|(_, v)| v.len() > 1)
            .map(|(k, v)| (v.len(), k))
            .collect();
        triple_counts.sort_unstable_by(|a, b| b.cmp(&a));
        println!("\n=== 3-point (d1, d2): count top30 ===");
        for (count, (d1, d2)) in triple_counts.iter().take(30) {
            println!("  ({d1:>4}, {d2:>4}): {count}");
        }
    }

    // ===== 4 点：距离向量 (d1, d2, d3) =====
    {
        let mut quads: HashMap<(Index, Index, Index), Vec<(Index, Tone)>> = HashMap::new();
        for (&tone, ticks) in &by_tone {
            for i in 0..ticks.len() {
                for j in i + 1..ticks.len() {
                    for k in j + 1..ticks.len() {
                        for l in k + 1..ticks.len() {
                            let d1 = ticks[j] - ticks[i];
                            let d2 = ticks[k] - ticks[j];
                            let d3 = ticks[l] - ticks[k];
                            quads
                                .entry((d1, d2, d3))
                                .or_default()
                                .push((ticks[i], tone));
                        }
                    }
                }
            }
        }
        let mut quad_counts: Vec<(usize, (Index, Index, Index))> = quads
            .into_iter()
            .filter(|(_, v)| v.len() > 1)
            .map(|(k, v)| (v.len(), k))
            .collect();
        quad_counts.sort_unstable_by(|a, b| b.cmp(&a));
        println!("\n=== 4-point (d1, d2, d3): count top20 ===");
        for (count, (d1, d2, d3)) in quad_counts.iter().take(20) {
            println!("  ({d1:>4}, {d2:>4}, {d3:>4}): {count}");
        }
    }
}
