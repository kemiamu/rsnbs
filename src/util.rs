use crate::{Index, Note, Notes, Position, Tick, Tone};
use itertools::Itertools;
use std::collections::{BTreeMap, HashMap};
use std::num::NonZero;

impl Notes {
    /// flattens notes into the TP (tick–tone) plane multiset,
    /// collapsing layers by summing counts per (tick, tone).
    pub fn to_tp_multiset(&self) -> HashMap<Tick, HashMap<Tone, NonZero<usize>>> {
        let mut tp: HashMap<Tick, HashMap<Tone, usize>> = HashMap::new();
        for (pos, note) in self.iter() {
            *tp.entry(pos.tick())
                .or_default()
                .entry(note.tone())
                .or_default() += 1;
        }
        tp.into_iter()
            .map(|(tick, tones)| {
                let tones = tones
                    .into_iter()
                    .filter_map(|(tone, count)| NonZero::new(count).map(|c| (tone, c)))
                    .collect();
                (tick, tones)
            })
            .collect()
    }

    /// builds point enumeration (PE) from note pairs
    #[deprecated]
    pub fn build_pe<T, F>(
        &self,
        loop_length: Option<Tick>,
        classify: F,
    ) -> HashMap<Tick, HashMap<T, HashMap<Tick, NonZero<usize>>>>
    where
        T: Eq + std::hash::Hash + Clone,
        F: Fn(&Note) -> T,
    {
        let mut pe: HashMap<Tick, HashMap<T, HashMap<Tick, NonZero<usize>>>> = Default::default();
        let half_loop_bound = loop_length.map(|l| (l, l / 2));

        let pair = |left: Tick, right: Tick| -> (Tick, Tick) {
            debug_assert!(left <= right && left != right);
            let Some((loop_len, half_bound)) = half_loop_bound else {
                return (left, right - left);
            };
            let forward = right - left;
            let (anchor_tick, offset) = match forward <= half_bound {
                true => (left, forward),
                false => (right, loop_len - forward),
            };
            (anchor_tick, offset)
        };

        for [(left_pos, left_note), (right_pos, right_note)] in self.iter().array_combinations() {
            let class = match classify(left_note) {
                class if class == classify(right_note) => class,
                _ => continue,
            };
            let (anchor_tick, offset) = pair(left_pos.tick(), right_pos.tick());

            pe.entry(offset)
                .or_default()
                .entry(class)
                .or_default()
                .entry(anchor_tick)
                .and_modify(|c| *c = c.saturating_add(1))
                .or_insert(NonZero::new(1).unwrap());
        }

        pe
    }

    /// separates notes into matched and unmatched groups via pattern matching.
    pub fn matches_by<F>(self, pattern: &[Index], song_length: Index, f: F) -> (Notes, Notes)
    where
        F: Fn(&Note, &Note) -> bool,
    {
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

            let base = candidates[i].pos.tick();
            let result = pattern.into_iter().try_fold(vec![], |mut indices, p| {
                let target = (base + p) % song_length;
                let found = candidates.iter().enumerate().find(|(_, p)| {
                    !p.is_matched && p.pos.tick() == target && f(&p.note, &candidates[i].note)
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
    pub fn group_match<F>(
        self,
        pattern: &[Index],
        song_length: Index,
        f: F,
    ) -> (MatchedGroups, Notes)
    where
        F: Fn(&Note, &Note) -> bool,
    {
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

            let base = candidates[i].pos.tick();
            let result = pattern.into_iter().try_fold(vec![], |mut indices, p| {
                let target = (base + p) % song_length;
                let found = candidates.iter().enumerate().find(|(_, c)| {
                    !c.is_matched && c.pos.tick() == target && f(&c.note, &candidates[i].note)
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
}

/// reassign layers across multiple note groups so they don't overlap.
pub fn reassign_layers<I, J>(slices: I) -> Notes
where
    I: IntoIterator<Item = J>,
    J: IntoIterator<Item = (Tick, Note)>,
{
    let mut base_layer: Tick = Default::default();
    let mut result: BTreeMap<Position, Note> = Default::default();

    for notes in slices {
        let mut prev_tick: Tick = Tick::MAX;
        let mut prev_layer: Index = Default::default();
        let mut layers: Index = Default::default();

        for (tick, note) in notes.into_iter().sorted_unstable() {
            prev_layer = if tick == prev_tick { prev_layer + 1 } else { 0 };
            layers = layers.max(prev_layer + 2);
            prev_tick = tick;
            result.insert(Position::new(tick, base_layer + prev_layer), note);
        }
        base_layer += layers;
    }

    result.into()
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
