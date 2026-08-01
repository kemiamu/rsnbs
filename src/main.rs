use clap::Parser;
#[cfg(feature = "unstable")]
use rsnbs::note::Tone;
use rsnbs::note::{Note, Notes};
use rsnbs::schematic::{MultiCompactLayout, MultiLinearLayout, StackedLinearLayout};
use rsnbs::schematic::{SchematicBuilder, TappedLayout, WithFloor};
use rsnbs::song::Song;
use rsnbs::types::{IntoTick, Tick};
#[cfg(feature = "unstable")]
use rsnbs::util::{TpPlane, TransEqClass, VectorTable};
use std::collections::BTreeMap;
#[cfg(feature = "unstable")]
use std::collections::BTreeSet;
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
    #[cfg(feature = "unstable")]
    Decompose(Decompose),
}

fn main() {
    match Cli::parse() {
        Cli::Compact(cmd) => cmd.run(),
        Cli::Linear(cmd) => cmd.run(),
        #[cfg(feature = "unstable")]
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
    /// Only place floor where blocks exist above (default: full coverage)
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
    /// Only place floor where blocks exist above (default: full coverage)
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

#[cfg(feature = "unstable")]
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

#[cfg(feature = "unstable")]
impl Decompose {
    fn run(self) {
        // 有向锚点集：p 属于 M 且 p+o 属于 M 的点，重数取 min
        fn anchors_of(plane: &TpPlane, o: Tick) -> TpPlane {
            let mut out = TpPlane::default();
            for (&(t, tone), &c) in plane.iter() {
                if let Some(&c2) = plane.get(&(t + o, tone)) {
                    *out.entry((t, tone)).or_default() += c.min(c2);
                }
            }
            out
        }

        // 公共锚点：交集，重数取 min
        fn intersect(a: &TpPlane, b: &TpPlane) -> TpPlane {
            let mut out = TpPlane::default();
            for (&p, &c) in a.iter() {
                if let Some(&c2) = b.get(&p) {
                    *out.entry(p).or_default() += c.min(c2);
                }
            }
            out
        }

        // K (+) S 展开覆盖的点数（decompose 的 pattern 语义）
        fn pattern_size(kernel: &TpPlane, offsets: &BTreeSet<NonZero<Tick>>) -> usize {
            let mut set: BTreeSet<(Tick, Tone)> = BTreeSet::new();
            for (&(t, tone), &c) in kernel.iter() {
                if c > 0 {
                    set.insert((t, tone));
                    for o in offsets {
                        set.insert((t + o.get(), tone));
                    }
                }
            }
            set.len()
        }

        let song = Song::open_nbs(&self.input).unwrap();
        let song_len = song.len();
        let total = song.notes.len();
        let all_plane = TpPlane::from_iter(song.notes.clone());

        // 候选偏移：向量表 (step=8) 的键 + 小节对齐
        let bar = (song.header.tempo * 4.0) as Tick;
        let vt = VectorTable::from_plane(&all_plane, NonZero::new(song_len), 8);
        let mut candidates: BTreeSet<NonZero<Tick>> =
            vt.keys().copied().filter_map(NonZero::new).collect();
        for m in 1..=8 {
            if let Some(o) = NonZero::new(bar * m) {
                if o.get() < song_len {
                    candidates.insert(o);
                }
            }
        }
        let candidates: Vec<NonZero<Tick>> = candidates.into_iter().collect();

        // 各候选偏移在原始乐谱上的锚点集
        let cov: BTreeMap<Tick, TpPlane> = candidates
            .iter()
            .map(|&o| (o.get(), anchors_of(&all_plane, o.get())))
            .collect();

        // 贪心：逐轮加入使 K (+) S 展开覆盖最大的偏移，
        // 锚点始终保持为所有已选偏移的公共交集，保证展开不产生谱外音符
        let mut selected: BTreeSet<NonZero<Tick>> = BTreeSet::new();
        let mut anchors = all_plane.clone();
        let mut best_size = 0usize;
        let mut rounds = 0usize;
        loop {
            let mut best: Option<(NonZero<Tick>, TpPlane, usize)> = None;
            for &o in &candidates {
                if selected.contains(&o) {
                    continue;
                }
                let a2 = intersect(&anchors, &cov[&o.get()]);
                if a2.is_empty() {
                    continue;
                }
                let mut s2 = selected.clone();
                s2.insert(o);
                let kernel = TransEqClass::new(s2.clone(), a2.clone()).into_pruned().1;
                let size = pattern_size(&kernel, &s2);
                if best.as_ref().map_or(true, |(_, _, s)| size > *s) {
                    best = Some((o, a2, size));
                }
            }
            let Some((o, a2, size)) = best else {
                break;
            };
            if size <= best_size {
                break;
            }
            best_size = size;
            selected.insert(o);
            anchors = a2;
            rounds += 1;
        }

        // 两层：匹配层（K (+) S）+ 残差层（R）
        let matched_tec = TransEqClass::new(selected.clone(), anchors);
        #[allow(deprecated)]
        let (pat, res) = matched_tec.decompose(&song.notes, song_len);
        eprintln!(
            "matched {} offsets {:?} in {} rounds: {}/{} notes ({:.1}%), residual {} notes",
            selected.len(),
            selected.iter().map(|o| o.get()).collect::<Vec<_>>(),
            rounds,
            pat.len(),
            total,
            pat.len() as f64 / total as f64 * 100.0,
            res.len(),
        );

        let mut tecs = vec![matched_tec];
        if !res.is_empty() {
            tecs.push(TransEqClass::new(BTreeSet::new(), TpPlane::from_iter(res)));
        }

        let layout = TappedLayout::new(tecs, NonZero::new(18), false);
        let litematic = SchematicBuilder(layout).build("Tapped from source.nbs", "rustnbs");
        litematic.write_file(&self.output).unwrap();
        eprintln!("Wrote {}", self.output);
    }
}
