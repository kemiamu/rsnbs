use crate::note::{Note, Notes};
use crate::types::{Index, LayerAnchor, Position, Tick, TimeAnchor};
use itertools::Itertools;
use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZero;

// Notes util
//
// ++++++++++++============++++++++++++============++++++++++++============

impl<A: TimeAnchor, E> Notes<A, E> {
    /// Rescales ticks from arbitrary tempo (tick/s) to standard game tick (20 t/s).
    pub fn rescale_to_game_tick(self, tempo: f32) -> impl Iterator<Item = (A, E)> {
        self.rescale_to_tick_rate(tempo, 20)
    }

    /// Rescales ticks from arbitrary tempo (tick/s) to redstone tick (10 t/s).
    pub fn rescale_to_redstone_tick(self, tempo: f32) -> impl Iterator<Item = (A, E)> {
        self.rescale_to_tick_rate(tempo, 10)
    }

    /// Rescales ticks from arbitrary tempo (tick/s) to the given target tick rate (t/s).
    pub fn rescale_to_tick_rate(
        self,
        tempo: f32,
        target_rate: u32,
    ) -> impl Iterator<Item = (A, E)> {
        // tempo outside (0, 30): assume NBS tick ≡ game tick, fold by target/20
        let scale = match (0.0..30.0).contains(&tempo) {
            true => target_rate as f32 / tempo,
            false => target_rate as f32 / 20.0,
        };
        // approximate scale to {z, 1/z} as (num, den), keeping tick transforms integral
        let (num, den) = match scale >= 1.0 {
            true => (scale.round() as u32, 1),
            false => (1, (1.0 / scale).round() as u32),
        };
        self.into_iter().map(move |(anchor, event)| {
            let tick = anchor.into_tick() * num / den;
            (anchor.with_tick(tick), event)
        })
    }
}

impl<A: LayerAnchor + Ord, E> Notes<A, E> {
    /// Groups notes into contiguous blocks separated by empty layers.
    pub fn split_by_layer_gaps(self) -> Vec<Notes<A, E>> {
        let layers: BTreeSet<Index> = self.keys().map(|pos| pos.into_layer()).collect();
        let block_start = |prev: &mut Option<Index>, curr: Index| {
            let keep = prev.map_or(true, |p| p + 1 != curr);
            *prev = Some(curr);
            Some(keep.then_some(curr))
        };
        let starts: Vec<Index> = layers
            .into_iter()
            .scan(None, block_start)
            .flatten()
            .collect();

        let mut groups: Vec<Notes<A, E>> = Vec::new();
        groups.resize_with(starts.len(), Notes::default);
        for (pos, note) in self {
            let idx = starts.partition_point(|&s| s <= pos.into_layer()) - 1;
            let pos = pos.with_layer(pos.into_layer() - starts[idx]);
            groups[idx].insert(pos, note);
        }
        groups
    }

    /// Splits notes into groups of `size` layers each.
    pub fn split_by_layer_count(self, size: Option<NonZero<usize>>) -> Vec<Notes<A, E>> {
        let Some(size) = size else {
            return vec![self];
        };
        let size = size.get();
        let mut groups: BTreeMap<Index, BTreeMap<A, E>> = BTreeMap::new();
        for (pos, note) in self {
            let group = pos.into_layer() / size as Index;
            let new_layer = pos.into_layer() % size as Index;
            let entry = groups.entry(group).or_default();
            entry.insert(pos.with_layer(new_layer), note);
        }
        groups.into_values().map(Notes::from).collect()
    }

    /// Stacks note groups vertically with 2 blank layers between, consuming them by value.
    pub fn concat<I: IntoIterator<Item = Notes<A, E>>>(notes: I) -> impl Iterator<Item = (A, E)> {
        let stacked = notes.into_iter().scan(0, |offset, n| {
            let base = *offset;
            let f = move |(pos, note): (A, E)| (pos.with_layer(pos.into_layer() + base), note);
            *offset += n.keys().map(|p| p.into_layer()).max().map_or(0, |m| m + 2);
            Some(n.into_iter().map(f))
        });
        stacked.flatten()
    }
}

// Experimental
//
// ++++++++++++============++++++++++++============++++++++++++============

impl Notes<Position, Note> {
    /// separates notes into matched and unmatched groups via pattern matching.
    pub fn matches_by<F: Fn(&Note, &Note) -> bool>(
        self,
        pattern: &[Tick],
        song_length: Tick,
        f: F,
    ) -> (Notes, Notes) {
        struct NoteWithMatch {
            pos: Position,
            note: Note,
            is_matched: bool,
        }
        let mut candidates: Vec<NoteWithMatch> = self
            .into_iter()
            .map(|(pos, note)| NoteWithMatch {
                pos,
                note,
                is_matched: false,
            })
            .collect();

        for i in 0..candidates.len() {
            if candidates[i].is_matched {
                continue;
            }

            let base = candidates[i].pos.into_tick();
            let result = pattern.into_iter().try_fold(vec![], |mut indices, p| {
                let target = (base + p) % song_length;
                let found = candidates.iter().enumerate().find(|(_, p)| {
                    !p.is_matched && p.pos.into_tick() == target && f(&p.note, &candidates[i].note)
                });
                found.map(|(idx, _)| {
                    indices.push(idx);
                    indices
                })
            });

            if let Some(indices) = result {
                for &idx in &indices {
                    candidates[idx].is_matched = true;
                }
            }
        }

        let (mut matched, mut unmatched) = (BTreeMap::new(), BTreeMap::new());
        for note in candidates {
            match note.is_matched {
                true => matched.insert(note.pos, note.note),
                false => unmatched.insert(note.pos, note.note),
            };
        }
        (matched.into(), unmatched.into())
    }

    /// like matches_by but preserves group boundaries, returns MatchedGroups.
    #[allow(deprecated)]
    pub fn group_match<F: Fn(&Note, &Note) -> bool>(
        self,
        pattern: &[Tick],
        song_length: Tick,
        f: F,
    ) -> (MatchedGroups, Notes) {
        struct Candidate {
            pos: Position,
            note: Note,
            is_matched: bool,
            group: usize,
        }

        let mut candidates: Vec<Candidate> = self
            .into_iter()
            .map(|(pos, note)| Candidate {
                pos,
                note,
                is_matched: false,
                group: 0,
            })
            .collect();

        let mut group_cnt = 0;
        for i in 0..candidates.len() {
            if candidates[i].is_matched {
                continue;
            }

            let base = candidates[i].pos.into_tick();
            let result = pattern.into_iter().try_fold(vec![], |mut indices, p| {
                let target = (base + p) % song_length;
                let found = candidates.iter().enumerate().find(|(_, c)| {
                    !c.is_matched && c.pos.into_tick() == target && f(&c.note, &candidates[i].note)
                });
                found.map(|(idx, _)| {
                    indices.push(idx);
                    indices
                })
            });

            if let Some(indices) = result {
                for &idx in &indices {
                    candidates[idx].is_matched = true;
                    candidates[idx].group = group_cnt;
                }
                group_cnt += 1;
            }
        }

        let mut groups: Vec<BTreeMap<Position, Note>> =
            (0..group_cnt).map(|_| BTreeMap::new()).collect();
        let mut unmatched = BTreeMap::new();
        for c in candidates {
            if c.is_matched {
                groups[c.group].insert(c.pos, c.note);
            } else {
                unmatched.insert(c.pos, c.note);
            }
        }

        (
            MatchedGroups {
                groups: groups.into_iter().map(Into::into).collect(),
            },
            unmatched.into(),
        )
    }

    /// reassign layers across multiple note groups so they don't overlap.
    #[deprecated(note = "use Notes::concat instead")]
    pub fn reassign_layers<I, J>(slices: I, gap: Index) -> Self
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = (Tick, Note)>,
    {
        let mut base_layer: Index = 0;
        let mut result: BTreeMap<Position, Note> = Default::default();

        for notes in slices {
            let mut prev_tick: Tick = Tick::MAX;
            let mut prev_layer: Index = Default::default();
            let mut layers: Index = 0;

            for (tick, note) in notes.into_iter().sorted_unstable() {
                prev_layer = if tick == prev_tick { prev_layer + 1 } else { 0 };
                layers = layers.max(prev_layer + 1 + gap);
                prev_tick = tick;
                result.insert(Position::new(tick, base_layer + prev_layer), note);
            }
            base_layer += layers;
        }

        result.into()
    }
}

/// pattern match result with group boundaries preserved.
/// each group corresponds to one complete pattern match.
#[deprecated(note = "this type is planned for deprecation")]
#[allow(deprecated)]
#[derive(Debug, Clone)]
pub struct MatchedGroups {
    groups: Vec<Notes>,
}

#[allow(deprecated)]
impl MatchedGroups {
    pub fn empty() -> Self {
        Self { groups: vec![] }
    }

    /// all matched groups, each group is all notes from one pattern match.
    pub fn groups(&self) -> &[Notes] {
        &self.groups
    }

    /// total number of matched notes.
    pub fn matched_len(&self) -> usize {
        self.groups.iter().map(|g| g.len()).sum()
    }

    /// template notes: first note of each group (base). for projection, one note per group.
    /// bases at different layers on the same tick are each preserved.
    pub fn templates(&self) -> Notes {
        self.groups
            .iter()
            .filter_map(|group| group.iter().next().map(|(p, n)| (*p, n.clone())))
            .collect()
    }
}
