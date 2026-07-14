use clap::Parser;
use rsnbs::layout::{LinearLayout, MultiCompactLayout};
use rsnbs::schematic::{SchematicBuilder, WithFloor};
use rsnbs::note::Note;
use rsnbs::song::Song;
use rsnbs::types::Tick;
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
}

fn main() {
    match Cli::parse() {
        Cli::Compact(cmd) => cmd.run(),
        Cli::Linear(cmd) => cmd.run(),
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
    /// Minimum repeat interval in game ticks, controls repeater granularity (0 = unlimited)
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
        let notes = song.notes.rescale(song.header.tempo);

        let mut by_tick: BTreeMap<Tick, Vec<Note>> = Default::default();
        for (pos, note) in notes {
            by_tick.entry(pos.tick()).or_default().push(note);
        }

        let tracks = std::iter::once((by_tick, NonZero::new(self.coarse)));
        let wrap = NonZero::new(self.wrap);
        let layout = MultiCompactLayout::new(tracks, wrap, self.gap);
        let description = format!("Sectional from {}", name);
        let litematic = match self.floor {
            true => SchematicBuilder(WithFloor(layout)).build(description, "rsnbs"),
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
        let layout = LinearLayout::new(
            song.notes.rescale(song.header.tempo).split_by_layer_gaps(),
            self.gap,
        );
        let description = format!("Sectional from {}", name);
        let litematic = match self.floor {
            true => SchematicBuilder(WithFloor(layout)).build(description, "rsnbs"),
            false => SchematicBuilder(layout).build(description, "rsnbs"),
        };
        litematic.write_file(&self.output).unwrap();
        eprintln!("Wrote {}", self.output);
    }
}
