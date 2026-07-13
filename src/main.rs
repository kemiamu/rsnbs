use clap::Parser;
use rsnbs::layout::{CompactLayout, LinearLayout};
use rsnbs::schematic::{SchematicBuilder, WithFloor};
use rsnbs::{Note, Notes, Position, Song, Tick};
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
/// Compact cursor-based layout
struct Compact {
    input: String,
    #[arg(default_value = "generated_compact.litematic")]
    output: String,
    #[arg(long, default_value_t = 16)]
    wrap: usize,
    #[arg(long, default_value_t = 0)]
    coarse: u32,
    #[arg(long, default_value_t = 0)]
    gap: u32,
    #[arg(long)]
    floor: bool,
}

impl Compact {
    fn run(self) {
        let song = Song::open_nbs(&self.input).unwrap();
        let name = self.input.clone();
        let notes = scale_notes(song.notes, song.header.tempo);

        let mut by_tick: BTreeMap<Tick, Vec<Note>> = Default::default();
        for (pos, note) in notes {
            by_tick.entry(pos.tick()).or_default().push(note);
        }

        let tracks = std::iter::once((by_tick, NonZero::new(self.coarse)));

        let wrap = NonZero::new(self.wrap);
        let layout = CompactLayout::new(tracks, wrap, self.gap);
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
    input: String,
    #[arg(default_value = "generated_linear.litematic")]
    output: String,
    #[arg(long, default_value_t = 0)]
    gap: u32,
    #[arg(long)]
    floor: bool,
}

impl Linear {
    fn run(self) {
        let song = Song::open_nbs(&self.input).unwrap();
        let name = self.input.clone();
        let notes = scale_notes(song.notes, song.header.tempo);
        let layout = LinearLayout::new(notes, self.gap);
        let description = format!("Sectional from {}", name);
        let litematic = match self.floor {
            true => SchematicBuilder(WithFloor(layout)).build(description, "rsnbs"),
            false => SchematicBuilder(layout).build(description, "rsnbs"),
        };
        litematic.write_file(&self.output).unwrap();
        eprintln!("Wrote {}", self.output);
    }
}

fn scale_notes(notes: Notes, tempo: f32) -> Notes {
    let scale = (20.0 / tempo).round() as u32;
    match scale > 1 {
        true => notes
            .into_iter()
            .map(|(pos, note)| (Position(pos.tick() * scale, pos.layer()), note))
            .collect(),
        false => notes,
    }
}
