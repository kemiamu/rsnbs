use clap::Parser;
use rsnbs::note::{Note, Notes};
use rsnbs::reuse::{plan_to_tecs, reuse_flow};
use rsnbs::schematic::{MultiCompactLayout, MultiLinearLayout, StackedLinearLayout};
use rsnbs::schematic::{SchematicBuilder, TappedLayout, WithFloor};
use rsnbs::song::Song;
use rsnbs::types::{IntoTick, Tick};
use rsnbs::util::TpPlane;
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
    #[arg(short, long, default_value_t = 16)]
    wrap: usize,
    /// Repeater delay coarseness 1-4 (0 = unlimited)
    #[arg(short, long, default_value_t = 0)]
    coarse: u32,
    /// Block spacing between adjacent tracks
    #[arg(short, long, default_value_t = 0)]
    gap: u32,
    /// Add a floor platform below the build
    #[arg(short, long)]
    floor: bool,
    /// Only place floor below gravity blocks (default: full coverage)
    #[arg(short, long)]
    sparse_floor: bool,
}

impl Compact {
    fn run(self) {
        let song = Song::open_nbs(&self.input).unwrap();
        let name = self.input.clone();
        let notes = song.notes.rescale_to_game_tick(song.header.tempo);

        let mut by_tick: BTreeMap<Tick, Vec<Note>> = Default::default();
        for (pos, note) in notes {
            by_tick.entry(pos.into_tick()).or_default().push(note);
        }

        let tracks = std::iter::once((by_tick, NonZero::new(self.coarse)));
        let wrap = NonZero::new(self.wrap);
        let layout = MultiCompactLayout::new(tracks, wrap, self.gap);
        let description = format!("Sectional from {}", name);
        let litematic = match self.floor || self.sparse_floor {
            true => SchematicBuilder(WithFloor::new(layout, !self.sparse_floor))
                .build(description, "rsnbs"),
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
    #[arg(short, long, default_value_t = 0)]
    gap: u32,
    /// Max columns per row before wrapping (0 = no wrap)
    #[arg(short, long, default_value_t = 0)]
    wrap: u32,
    /// Add a floor platform below the build (only when wrap = 0)
    #[arg(short, long)]
    floor: bool,
    /// Only place floor below gravity blocks (default: full coverage)
    #[arg(short, long)]
    sparse_floor: bool,
}

impl Linear {
    fn run(self) {
        let song = Song::open_nbs(&self.input).unwrap();
        let name = self.input.clone();
        let tracks: Vec<Notes> = song
            .notes
            .rescale_to_game_tick(song.header.tempo)
            .split_by_layer_gaps()
            .into_iter()
            .flat_map(|notes| notes.split_by_layer_count(NonZero::new(3)))
            .collect();
        let description = format!("Sectional from {}", name);
        let author = "rsnbs";

        let litematic = if let Some(wrap) = NonZero::new(self.wrap) {
            let layout = StackedLinearLayout::new(tracks, Some(wrap), self.gap, !self.sparse_floor);
            SchematicBuilder(layout).build(description, author)
        } else if self.floor || self.sparse_floor {
            let layout = MultiLinearLayout::new(tracks, self.gap);
            SchematicBuilder(WithFloor::new(layout, !self.sparse_floor)).build(description, author)
        } else {
            let layout = MultiLinearLayout::new(tracks, self.gap);
            SchematicBuilder(layout).build(description, author)
        };

        litematic.write_file(&self.output).unwrap();
        eprintln!("Wrote {}", self.output);
    }
}

// Decompose
//
// ++++++++++++============++++++++++++============++++++++++++============

#[derive(clap::Args)]
/// Decompose an NBS song into a matched layer and a residual layer,
/// projected as a tapped delay line layout.
struct Decompose {
    /// Path to input NBS file
    input: String,
    /// Path to output litematic file
    #[arg(default_value = "generated_tapped.litematic")]
    output: String,
}

impl Decompose {
    fn run(self) {
        // 复用流：族深先分层 + 族仲裁（wf_0813_reuse 移植）
        let song = Song::open_nbs(&self.input).unwrap();
        let all_plane = TpPlane::from_iter(song.notes.clone());

        let (plan, total_reuse, residual) = reuse_flow(&all_plane, 4, 4);

        eprintln!(
            "total reuse = {total_reuse}, residual events = {}",
            residual.values().sum::<usize>()
        );
        for layer in &plan {
            eprintln!(
                "  {:?}: K={} (sum {}) reuse={}",
                layer.scatter,
                layer.kernel.len(),
                layer.kernel.values().sum::<usize>(),
                layer.reuse(),
            );
        }

        // 物化适配：延迟线最小间距限制内的层进入 TEC，其余退回残差
        let (tecs, skipped) = plan_to_tecs(plan, residual);
        if skipped > 0 {
            eprintln!("  {skipped} layer(s) skipped: min gap < 8");
        }

        let layout = TappedLayout::new(tecs, NonZero::new(18), false);
        let litematic = SchematicBuilder(layout).build("Tapped from source.nbs", "rsnbs");
        litematic.write_file(&self.output).unwrap();
        eprintln!("Wrote {}", self.output);
    }
}
