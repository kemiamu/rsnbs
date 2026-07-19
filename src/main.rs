use clap::Parser;
use rsnbs::layout::{MultiCompactLayout, MultiLinearLayout};
use rsnbs::note::{Note, Notes};
use rsnbs::schematic::{SchematicBuilder, WithFloor};
use rsnbs::song::Song;
use rsnbs::types::Tick;
use rsnbs::util::{TpPlane, VectorTable};
use std::collections::BTreeMap;
use std::num::NonZero;

// Cli
//
// ++++++++++++============++++++++++++============++++++++++++============

#[derive(Parser)]
#[command(
    name = "rsnbs",
    about = "Generate Minecraft litematic projections from NBS songs"
)]
enum Cli {
    Compact(Compact),
    Linear(Linear),
    Decompose(Decompose),
}

fn main() {
    match Cli::parse() {
        Cli::Compact(cmd) => cmd.run(),
        Cli::Linear(cmd) => cmd.run(),
        Cli::Decompose(cmd) => cmd.run(),
    }
}

// Compact
//
// ++++++++++++============++++++++++++============++++++++++++============

#[derive(clap::Args)]
/// Compact layout
struct Compact {
    /// Path to input NBS file
    input: String,
    /// Path to output litematic file
    #[arg(default_value = "generated_compact.litematic")]
    output: String,
    /// Max tiles per row before wrapping (0 = no wrap)
    #[arg(long, default_value_t = 16)]
    wrap: usize,
    /// Repeater delay coarseness 1-4 (0 = unlimited)
    #[arg(long, default_value_t = 0)]
    coarse: u32,
    /// Block spacing between adjacent tracks
    #[arg(long, default_value_t = 0)]
    gap: u32,
    /// Add a floor platform below the build
    #[arg(long)]
    floor: bool,
}

impl Compact {
    fn run(self) {
        let song = Song::open_nbs(&self.input).unwrap();
        let name = self.input.clone();
        let notes = song.notes.rescale_to_game_tick(song.header.tempo);

        let mut by_tick: BTreeMap<Tick, Vec<Note>> = Default::default();
        for (pos, note) in notes {
            by_tick.entry(pos.tick()).or_default().push(note);
        }

        let tracks = std::iter::once((by_tick, NonZero::new(self.coarse)));
        let wrap = NonZero::new(self.wrap);
        let layout = MultiCompactLayout::new(tracks, wrap, self.gap);
        let description = format!("Sectional from {}", name);
        let litematic = match self.floor {
            true => SchematicBuilder(WithFloor::new(layout, true)).build(description, "rsnbs"),
            false => SchematicBuilder(layout).build(description, "rsnbs"),
        };
        litematic.write_file(&self.output).unwrap();
        eprintln!("Wrote {}", self.output);
    }
}

// Linear
//
// ++++++++++++============++++++++++++============++++++++++++============

#[derive(clap::Args)]
/// Linear time-proportional layout
struct Linear {
    /// Path to input NBS file
    input: String,
    /// Path to output litematic file
    #[arg(default_value = "generated_linear.litematic")]
    output: String,
    /// Block spacing between adjacent tracks
    #[arg(long, default_value_t = 0)]
    gap: u32,
    /// Add a floor platform below the build
    #[arg(long)]
    floor: bool,
}

impl Linear {
    fn run(self) {
        let song = Song::open_nbs(&self.input).unwrap();
        let name = self.input.clone();
        let tracks = song
            .notes
            .rescale_to_game_tick(song.header.tempo)
            .split_by_layer_gaps()
            .into_iter()
            .flat_map(|notes| notes.split_by_layer_count(NonZero::new(3)))
            .collect();
        let layout = MultiLinearLayout::new(tracks, self.gap);
        let description = format!("Sectional from {}", name);
        let litematic = match self.floor {
            true => SchematicBuilder(WithFloor::new(layout, true)).build(description, "rsnbs"),
            false => SchematicBuilder(layout).build(description, "rsnbs"),
        };
        litematic.write_file(&self.output).unwrap();
        eprintln!("Wrote {}", self.output);
    }
}

// Decompose
//
// ++++++++++++============++++++++++++============++++++++++++============

#[derive(clap::Args)]
/// Decompose an NBS song into pattern and residual notes using FP-Growth.
struct Decompose {
    /// Path to input NBS file
    input: String,
    /// Path to output NBS file (pattern on top, residual below)
    #[arg(default_value = "decomposed.nbs")]
    output: String,
    /// FP-Growth min_support (0.0–1.0), higher = smaller/stronger patterns
    #[arg(long, default_value_t = 0.3)]
    min_support: f64,
}

impl Decompose {
    fn run(self) {
        let song = Song::open_nbs(&self.input).unwrap();
        let song_len = song.len();
        let mut remaining = song.notes.clone();
        let total = song.notes.len();

        // 多级步长：先大粒度提取，再逐步细化
        let steps: &[Tick] = &[4, 2, 1];
        let mut patterns: Vec<(Notes, Tick)> = vec![];
        let mut infos: Vec<String> = vec![];

        for &step in steps {
            let plane = TpPlane::from(remaining.clone());
            let vt = VectorTable::from_plane(&plane, NonZero::new(song_len), step);

            if let Some(tec) = vt.find_largest_tec(3) {
                let offsets: Vec<Tick> = tec.offsets().iter().map(|o| o.get()).collect();
                let n_anchors = tec.points().len();
                let (pat, res) = tec.decompose(&remaining, song_len);

                if pat.len() > 0 {
                    patterns.push((pat, step));
                    infos.push(format!(
                        "  Step {}: offsets {:?}, {} anchors → {} pattern notes",
                        step,
                        offsets,
                        n_anchors,
                        patterns.last().unwrap().0.len(),
                    ));
                    remaining = res;
                }
            }
        }

        let pattern_total: usize = patterns.iter().map(|(p, _)| p.len()).sum();
        let residual_total = remaining.len();

        // 构建多轨组装：每个 pattern 一轨 + residual 最后一轨
        let mut all_layers: Vec<Vec<(Tick, Note)>> = patterns
            .iter()
            .map(|(pat, _)| {
                pat.iter()
                    .map(|(pos, note)| (pos.tick(), note.clone()))
                    .collect()
            })
            .collect();
        all_layers.push(
            remaining
                .iter()
                .map(|(pos, note)| (pos.tick(), note.clone()))
                .collect(),
        );

        let rebuilt = Notes::reassign_layers(all_layers, 1);

        let mut out = Song::new();
        out.notes = rebuilt;
        out.refresh();
        out.save_nbs(&self.output).unwrap();

        // 打印结果
        for info in &infos {
            eprintln!("{}", info);
        }
        eprintln!(
            "  Pattern total: {:>5} ({:.1}%)\n  Residual total: {:>5} ({:.1}%)\n  Overall:        {:>5}",
            pattern_total,
            pattern_total as f64 / total as f64 * 100.0,
            residual_total,
            residual_total as f64 / total as f64 * 100.0,
            total,
        );
        eprintln!("Wrote {}", self.output);
    }
}
