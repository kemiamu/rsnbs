use clap::{Parser, Subcommand};
use rsnbs::schematic::SchematicBuilder;
use rsnbs::util::NotesExt;
use rsnbs::{Index, Note, Position, Song};
use std::collections::{BTreeMap, BTreeSet};
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
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Command::Matching { input, output } => matching(&input, &output),
    }
}

// cargo run -- matching fixtures/source.nbs

fn matching(input: &str, output: &str) {
    let song = Song::open_nbs(input).unwrap();
    let notes = song.notes.clone();

    // hardcoded parameters
    let song_length: Index = 1024;
    let min_notes: usize = 0;
    let coarse: Index = 4;
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
        &[0, 32, 192, 224, 256, 288, 320, 352, 384, 416, 448, 480],
        &[0, 32, 64, 96, 192, 256, 288, 320, 352, 384, 416, 448, 480],
        &[0, 192, 256, 288, 320, 352, 384, 416],
    ];
    let sectional_patterns: &[&[Index]] = &[&[0, 16, 32, 48], &[0, 16], &[0]];
    let sections: &[Range<Index>] = &[0..256, 256..512];

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
    let stem = std::path::Path::new(input)
        .file_stem()
        .unwrap()
        .to_str()
        .unwrap();
    let nbs_path = std::path::Path::new(output).join(format!("{}.nbs", stem));
    let litematic_path = std::path::Path::new(output).join(format!("{}.litematic", stem));

    std::fs::create_dir_all(output).unwrap();

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
