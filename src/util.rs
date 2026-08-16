use crate::note::{Note, Notes, Tone};
use crate::types::{Index, IntoTick, Position, Tick};
use counter::Counter;
use itertools::{Itertools, iproduct};
use std::collections::{BTreeMap, BTreeSet};
use std::iter::repeat;
use std::num::NonZero;
use std::ops::{BitAnd, Deref, DerefMut};

// TpPlane
//
// ++++++++++++============++++++++++++============++++++++++++============

/// A point in the TP (tick-tone) plane.
pub type Point = (Tick, Tone);

/// TP (tick-tone) plane multiset.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TpPlane(Counter<Point>);

impl TpPlane {
    /// Expand into an iterator of individual `(Tick, Tone)` points.
    pub fn into_points(self) -> impl Iterator<Item = Point> {
        let TpPlane(inner) = self;
        inner
            .into_iter()
            .flat_map(|(point, count)| repeat(point).take(count))
    }
}

impl<K: IntoTick, V: Into<Tone>> FromIterator<(K, V)> for TpPlane {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        let inner = iter
            .into_iter()
            .map(|(k, v)| (k.into_tick(), v.into()))
            .collect();
        Self(inner)
    }
}

impl From<TpPlane> for Notes<Tick, Vec<Tone>> {
    fn from(plane: TpPlane) -> Self {
        let mut by_tick = plane.into_points().into_group_map();
        for tones in by_tick.values_mut() {
            tones.sort();
        }
        Self::from_iter(by_tick)
    }
}

impl Deref for TpPlane {
    type Target = Counter<Point>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for TpPlane {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

// Translation Equivalence Class
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Translation Equivalence Class (TEC)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransEqClass {
    /// Offsets defining the translation pattern.
    offsets: BTreeSet<NonZero<Tick>>,
    /// Points (multiset) that this TEC operates on.
    points: TpPlane,
}

impl TransEqClass {
    pub fn new(offsets: BTreeSet<NonZero<Tick>>, points: TpPlane) -> Self {
        Self { offsets, points }
    }

    /// The offsets that define this translation pattern.
    pub fn offsets(&self) -> &BTreeSet<NonZero<Tick>> {
        &self.offsets
    }

    /// The anchor points common to all offsets.
    pub fn points(&self) -> &TpPlane {
        &self.points
    }

    /// Consume the TEC and return `(offsets, pruned_kernel)`.
    pub fn into_pruned(self) -> (BTreeSet<NonZero<Tick>>, TpPlane) {
        let offsets = self.offsets;
        let mut points = self.points;
        let indexes: Vec<Point> = points.keys().copied().sorted().collect();

        for (point @ (tick, tone), scatter) in iproduct!(indexes, offsets.iter()) {
            let shifted = &(tick + scatter.get(), tone);
            let anchor_mult = points[&point];
            let entry = points.entry(*shifted);
            entry.and_modify(|mult| *mult -= anchor_mult.min(*mult));
        }
        (offsets, points)
    }
}

impl BitAnd for TransEqClass {
    fn bitand(self, rhs: Self) -> Self {
        let offsets = &self.offsets | &rhs.offsets;
        let points = TpPlane(self.points.0 & rhs.points.0);
        Self { offsets, points }
    }
    type Output = Self;
}

impl<const N: usize> From<([NonZero<Tick>; N], TpPlane)> for TransEqClass {
    fn from((offsets, points): ([NonZero<Tick>; N], TpPlane)) -> Self {
        let offsets = BTreeSet::from(offsets);
        Self { offsets, points }
    }
}

// Notes util
//
// ++++++++++++============++++++++++++============++++++++++++============

impl Notes<Position, Note> {
    /// Rescales ticks from arbitrary tempo (tick/s) to standard game tick (20 t/s).
    pub fn rescale_to_game_tick(self, tempo: f32) -> Notes {
        self.rescale_to_tick_rate(tempo, 20)
    }

    /// Rescales ticks from arbitrary tempo (tick/s) to redstone tick (10 t/s).
    pub fn rescale_to_redstone_tick(self, tempo: f32) -> Notes {
        self.rescale_to_tick_rate(tempo, 10)
    }

    /// Rescales ticks from arbitrary tempo (tick/s) to the given target tick rate (t/s).
    pub fn rescale_to_tick_rate(self, tempo: f32, target_rate: u32) -> Notes {
        let scale = (target_rate as f32 / tempo).round() as u32;
        let map_pos = |pos: Position| Position::new(pos.into_tick() * scale, pos.layer());
        match scale > 1 {
            true => self.into_iter().map(|(p, n)| (map_pos(p), n)).collect(),
            false => self,
        }
    }

    /// Groups notes into contiguous blocks separated by empty layers.
    pub fn split_by_layer_gaps(self) -> Vec<Notes> {
        let layers: BTreeSet<Index> = self.keys().map(|pos| pos.layer()).collect();
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

        let mut groups: Vec<Notes> = vec![Default::default(); starts.len()];
        for (pos, note) in self {
            let idx = starts.partition_point(|&s| s <= pos.layer()) - 1;
            let pos = Position::new(pos.into_tick(), pos.layer() - starts[idx]);
            groups[idx].insert(pos, note);
        }
        groups
    }

    /// Splits notes into groups of `size` layers each.
    pub fn split_by_layer_count(self, size: Option<NonZero<usize>>) -> Vec<Notes> {
        let Some(size) = size else {
            return vec![self];
        };
        let size = size.get();
        let mut groups: BTreeMap<Index, BTreeMap<Position, Note>> = BTreeMap::new();
        for (pos, note) in self {
            let group = pos.layer() / size as Index;
            let new_layer = pos.layer() % size as Index;
            let entry = groups.entry(group).or_default();
            entry.insert(Position::new(pos.into_tick(), new_layer), note);
        }
        groups.into_values().map(Notes::from).collect()
    }

    /// Concatenate multiple note groups with blank layer separators.
    pub fn concat<'a, I: IntoIterator<Item = &'a Notes>>(notes: I) -> Self {
        let shift = |(pos, note): (&Position, &Note), base: Index| {
            (
                Position::new(pos.into_tick(), pos.layer() + base),
                note.clone(),
            )
        };
        let mut offset = 0;
        let stacked = notes.into_iter().flat_map(|n| {
            let base = offset.clone();
            offset += n.keys().map(|p| p.layer()).max().map_or(0, |m| m + 2);
            n.iter().map(move |pair| shift(pair, base))
        });
        stacked.collect()
    }

    // Experimental
    //
    // ++++++++++++============++++++++++++============++++++++++++============

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
