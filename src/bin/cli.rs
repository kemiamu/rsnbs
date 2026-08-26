use clap::Parser;
use rsnbs::analysis::reuse::{plan_to_tecs, reuse_flow};
use rsnbs::analysis::{BoundedTec, TePlane, TransEqClass};
use rsnbs::note::{Note, Notes, Tone};
use rsnbs::schematic::{Layout, MultiCompactLayout, MultiLinearLayout};
use rsnbs::schematic::{StackedLinearLayout, TappedLayout, WithFloor};
use rsnbs::song::Song;
use rsnbs::types::{Tick, TimeAnchor};
use rustmatica::Litematic;
use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZero;
use std::path::Path;
use std::str::FromStr;

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
    Match(Match),
}

fn main() {
    match Cli::parse() {
        Cli::Compact(cmd) => cmd.run(),
        Cli::Linear(cmd) => cmd.run(),
        Cli::Decompose(cmd) => cmd.run(),
        Cli::Match(cmd) => cmd.run(),
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
    /// Floor platform mode
    #[arg(short, long, value_enum, default_value_t = Floor::None)]
    floor: Floor,
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
        let litematic = build_schematic(layout, self.floor, description);
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
    /// Floor platform mode
    #[arg(short, long, value_enum, default_value_t = Floor::None)]
    floor: Floor,
}

impl Linear {
    fn run(self) {
        let song = open_song(&self.input);
        let tracks: Vec<BTreeMap<Tick, Vec<Tone>>> = song
            .notes
            .rescale_to_game_tick(song.header.tempo)
            .collect::<Notes>()
            .split_by_layer_gaps()
            .into_iter()
            .flat_map(|notes| notes.split_by_layer_count(NonZero::new(3)))
            .map(group_by_tick)
            .collect();
        let description = format!("Sectional from {}", self.input);

        let litematic = if let Some(wrap) = NonZero::new(self.wrap) {
            let layout = StackedLinearLayout::new(tracks, Some(wrap), self.gap, self.floor.full());
            build_schematic(layout, Floor::None, description)
        } else {
            let layout = MultiLinearLayout::new(tracks, self.gap);
            build_schematic(layout, self.floor, description)
        };
        write_output(&self.output, litematic);
    }
}

fn group_by_tick(notes: Notes) -> BTreeMap<Tick, Vec<Tone>> {
    notes
        .into_iter()
        .fold(BTreeMap::new(), |mut chords, (pos, note)| {
            chords.entry(pos.into_tick()).or_default().push(note.into());
            chords
        })
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
    #[arg(short, long, default_value_t = 2)]
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
        let all_plane: TePlane<Tone> =
            TePlane::from_iter(song.notes.rescale_to_redstone_tick(song.header.tempo));

        // 层数预算 = 布局高度的物理替身：分解在预算耗尽时停止；
        // 0 = 无预算，持续到自然极限（残差无任何同音色配对），层数可能远超布局可行范围
        let max_layers = match self.layers {
            0 => usize::MAX,
            n => n,
        };
        let (plan, _, residual) = reuse_flow(&all_plane, 6, max_layers);

        // 物化适配：延迟线最小间距限制内的层进入 TEC，其余退回残差
        let (tecs, _) = plan_to_tecs(plan, residual);

        let layout = TappedLayout::new(
            tecs.into_iter().map(BoundedTec::new),
            NonZero::new(self.wrap),
            self.floor,
        );
        let description = format!("Tapped from {}", self.input);
        let litematic = build_schematic(layout, Floor::None, description);
        write_output(&self.output, litematic);
    }
}

// Match
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Manually specified TEC offsets applied as reuse layers.
#[derive(clap::Args)]
struct Match {
    /// Path to input NBS file
    input: String,
    /// Path to output litematic file
    #[arg(default_value = "out/generated_match.litematic")]
    output: String,
    /// Match rule offsets, slash-separated; multiple rules in order
    #[arg(short, long, num_args = 1..)]
    rules: Vec<Rule>,
    /// Max tiles per row before wrapping (0 = no wrap)
    #[arg(short, long, default_value_t = 16)]
    wrap: usize,
    /// Add a full floor platform below the build (default: floor only below gravity blocks)
    #[arg(short, long)]
    floor: bool,
}

impl Match {
    fn run(self) {
        let song = open_song(&self.input);
        let mut residual: TePlane<Tone> =
            TePlane::from_iter(song.notes.rescale_to_redstone_tick(song.header.tempo));

        // normalize rules.
        let rules = self.rules.into_iter().map(|Rule(mut scatter)| {
            scatter.sort_unstable();
            scatter.dedup();
            scatter
        });

        // one TEC each.
        let mut tecs: Vec<BoundedTec<Tone>> = Vec::new();
        for rule in rules {
            let scatter: BTreeSet<_> = rule.into_iter().filter_map(NonZero::new).collect();
            tecs.push(BoundedTec::extract_from(&mut residual, scatter));
        }

        // keep the rest.
        if !residual.is_empty() {
            let offsets = Default::default();
            tecs.push(BoundedTec::new(TransEqClass::new(offsets, residual)));
        }

        let layout = TappedLayout::new(tecs, NonZero::new(self.wrap), self.floor);
        let description = format!("Match from {}", self.input);
        let litematic = build_schematic(layout, Floor::None, description);
        write_output(&self.output, litematic);
    }
}

/// One match rule's offsets, slash-separated, e.g. "4/8".
#[derive(Clone, Debug)]
struct Rule(Vec<Tick>);

impl FromStr for Rule {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parse = |t: &str| t.parse().map_err(|_| format!("invalid tick: {t}"));
        let tokens = s.split(|c| matches!(c, '/' | ',' | ' '));
        let offsets: Result<_, _> = tokens.filter(|t| !t.is_empty()).map(parse).collect();
        Ok(Rule(offsets?))
    }
}

// Utils
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Builds the schematic, wrapping the layout in a floor platform when requested.
fn build_schematic<L: Layout>(layout: L, floor: Floor, description: String) -> Litematic {
    const AUTHOR: &str = "rsnbs";
    match floor {
        Floor::None => layout.as_litematic(description, AUTHOR),
        _ => WithFloor::new(layout, floor.full()).as_litematic(description, AUTHOR),
    }
}

/// Floor platform mode.
#[derive(Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
enum Floor {
    /// No floor platform.
    #[default]
    None,
    /// Full coverage platform.
    Full,
    /// Platform only below gravity blocks.
    Gravity,
}

impl Floor {
    /// Full-coverage flag for layouts that always carry a floor.
    fn full(self) -> bool {
        matches!(self, Floor::Full | Floor::None)
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
