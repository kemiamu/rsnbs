use crate::{Index, Note, Notes, Position};
use std::collections::BTreeMap;

impl Notes {
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

    /// reassign layers across multiple note groups so they don't overlap.
    pub fn reassign_layers<I, J>(slices: I) -> Notes
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = (Index, Note)>,
    {
        let mut base_layer: Index = Default::default();
        let mut result: BTreeMap<Position, Note> = Default::default();

        for notes in slices {
            let mut notes: Vec<(Index, Note)> = notes.into_iter().collect();
            notes.sort_unstable_by_key(|(tick, _)| *tick);
            let mut notes = notes
                .into_iter()
                .map(|(tick, note)| (Position::new(tick, 0), note))
                .peekable();

            let mut current_layer: Index = Default::default();
            let mut max_layer: Index = Default::default();

            while let Some((pos, note)) = notes.next() {
                let pos = Position::new(pos.tick(), base_layer + current_layer);
                max_layer = max_layer.max(current_layer + 2);
                match notes.peek().map(|(p, _)| p.tick()) == Some(pos.tick()) {
                    true => current_layer += 1,
                    false => current_layer = 0,
                }
                result.insert(pos, note);
            }
            base_layer += max_layer;
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
