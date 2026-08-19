use clap::Parser;
use rsnbs::note::{Note, Notes};
use rsnbs::reuse::{plan_to_tecs, reuse_flow};
use rsnbs::schematic::{Layout, MultiCompactLayout, MultiLinearLayout, SchematicBuilder};
use rsnbs::schematic::{StackedLinearLayout, TappedLayout, WithFloor};
use rsnbs::song::Song;
use rsnbs::types::{Tick, TickAnchor};
use rsnbs::util::TpPlane;
use rustmatica::Litematic;
use std::collections::BTreeMap;
use std::num::NonZero;
use std::path::Path;

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

/// Compact layout
#[derive(clap::Args)]
struct Compact {
    /// Path to input NBS file
    input: String,
    /// Path to output litematic file
    #[arg(default_value = "out/generated_compact.litematic")]
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
        let song = open_song(&self.input);
        let notes = song.notes.rescale_to_game_tick(song.header.tempo);

        let mut by_tick: BTreeMap<Tick, Vec<Note>> = Default::default();
        for (pos, note) in notes {
            by_tick.entry(pos.into_tick()).or_default().push(note);
        }

        let tracks = std::iter::once((by_tick, NonZero::new(self.coarse)));
        let layout = MultiCompactLayout::new(tracks, NonZero::new(self.wrap), self.gap);
        let description = format!("Sectional from {}", self.input);
        let litematic = build_schematic(layout, self.floor, self.sparse_floor, description);
        write_output(&self.output, litematic);
    }
}

// Linear
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Linear time-proportional layout
#[derive(clap::Args)]
struct Linear {
    /// Path to input NBS file
    input: String,
    /// Path to output litematic file
    #[arg(default_value = "out/generated_linear.litematic")]
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
        let song = open_song(&self.input);
        let tracks: Vec<Notes> = song
            .notes
            .rescale_to_game_tick(song.header.tempo)
            .collect::<Notes>()
            .split_by_layer_gaps()
            .into_iter()
            .flat_map(|notes| notes.split_by_layer_count(NonZero::new(3)))
            .collect();
        let description = format!("Sectional from {}", self.input);

        let litematic = if let Some(wrap) = NonZero::new(self.wrap) {
            let layout = StackedLinearLayout::new(tracks, Some(wrap), self.gap, !self.sparse_floor);
            build_schematic(layout, false, false, description)
        } else {
            let layout = MultiLinearLayout::new(tracks, self.gap);
            build_schematic(layout, self.floor, self.sparse_floor, description)
        };
        write_output(&self.output, litematic);
    }
}

// Decompose
//
// ++++++++++++============++++++++++++============++++++++++++============

// **Experimental**: output may change.

/// Decompose an NBS song into TEC layers and a residual (tapped delay line).
#[derive(clap::Args)]
struct Decompose {
    /// Path to input NBS file
    input: String,
    /// Path to output litematic file
    #[arg(default_value = "out/generated_tapped.litematic")]
    output: String,
    /// Max number of layers (TECs) to generate; 0 = no budget
    #[arg(short, long, default_value_t = 3)]
    layers: usize,
    /// Max tiles per row before wrapping (0 = no wrap)
    #[arg(short, long, default_value_t = 16)]
    wrap: usize,
    /// Add a full floor platform below the build (default: floor only below gravity blocks)
    #[arg(short, long)]
    floor: bool,
}

impl Decompose {
    fn run(self) {
        let song = open_song(&self.input);
        // 统一 tempo 到红石刻 (10tps)
        let all_plane = TpPlane::from_iter(song.notes.rescale_to_redstone_tick(song.header.tempo));

        // 层数预算 = 布局高度的物理替身：分解在预算耗尽时停止；
        // 0 = 无预算，持续到自然极限（残差无任何同音色配对），层数可能远超布局可行范围
        let max_layers = match self.layers {
            0 => usize::MAX,
            n => n,
        };
        let (plan, total_reuse, residual) = reuse_flow(&all_plane, 6, max_layers);

        let residual_events = residual.values().sum::<usize>();
        eprintln!("total reuse = {total_reuse}, residual events = {residual_events}");

        // 物化适配：延迟线最小间距限制内的层进入 TEC，其余退回残差
        let (tecs, skipped) = plan_to_tecs(plan, residual);
        if skipped > 0 {
            eprintln!("  {skipped} layer(s) skipped: min gap < 8");
        }

        let layout = TappedLayout::new(tecs, NonZero::new(self.wrap), self.floor);
        let description = format!("Tapped from {}", self.input);
        let litematic = build_schematic(layout, false, false, description);
        write_output(&self.output, litematic);
    }
}

// Utils
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Builds the schematic, wrapping the layout in a floor platform when requested.
fn build_schematic<L: Layout>(
    layout: L,
    floor: bool,
    sparse_floor: bool,
    description: String,
) -> Litematic {
    const AUTHOR: &str = "rsnbs";
    match floor || sparse_floor {
        true => SchematicBuilder(WithFloor::new(layout, !sparse_floor)).build(description, AUTHOR),
        false => SchematicBuilder(layout).build(description, AUTHOR),
    }
}

/// Loads the input song.
fn open_song(input: &str) -> Song {
    Song::open_nbs(input).unwrap()
}

/// Ensures the parent directory exists, writes the litematic, and reports it.
fn write_output(output: &str, litematic: Litematic) {
    let parent = Path::new(output)
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty());
    if let Some(dir) = parent {
        std::fs::create_dir_all(dir).unwrap();
    }
    litematic.write_file(output).unwrap();
    eprintln!("Wrote {output}");
}
