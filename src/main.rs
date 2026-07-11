use clap::Parser;
use rsnbs::layout::{CompactLayout, LinearLayout};
use rsnbs::schematic::SchematicBuilder;
use rsnbs::{Note, Notes, Position, Song, Tick};
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
    #[arg(long, default_value = None)]
    wrap: Option<usize>,
    #[arg(long, default_value_t = 4)]
    coarse: u32,
    #[arg(long, default_value_t = 0)]
    gap: u32,
}

impl Compact {
    fn run(self) {
        let song = Song::open_nbs(&self.input).unwrap();
        let name = self.input.clone();
        let notes = scale_notes(song.notes, song.header.tempo);

        let mut by_layer: Vec<Vec<(Tick, Note)>> = Vec::new();
        for (pos, note) in notes {
            let layer = pos.layer() as usize;
            while by_layer.len() <= layer {
                by_layer.push(Vec::new());
            }
            by_layer[layer].push((pos.tick(), note));
        }

        let tracks = by_layer.into_iter().map(|notes| {
            let mut map: std::collections::BTreeMap<Tick, Vec<Note>> =
                std::collections::BTreeMap::new();
            for (tick, note) in notes {
                map.entry(tick).or_default().push(note);
            }
            (map, NonZero::new(self.coarse))
        });

        let wrap = self.wrap.and_then(NonZero::new);
        let layout = CompactLayout::new(tracks, wrap, self.gap);
        let litematic = SchematicBuilder(layout).build(name, "rsnbs");
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
}

impl Linear {
    fn run(self) {
        let song = Song::open_nbs(&self.input).unwrap();
        let name = self.input.clone();
        let notes = scale_notes(song.notes, song.header.tempo);
        let layout = LinearLayout::new(notes, self.gap);
        let litematic = SchematicBuilder(layout).build(name, "rsnbs");
        litematic.write_file(&self.output).unwrap();
        eprintln!("Wrote {}", self.output);
    }
}

fn scale_notes(notes: Notes, tempo: f32) -> Notes {
    let scale = (20.0 / tempo).round() as u32;
    match scale > 1 {
        true => notes
            .into_iter()
            .map(|(pos, note)| (Position::new(pos.tick() * scale, pos.layer()), note))
            .collect(),
        false => notes,
    }
}
