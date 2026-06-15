use crate::{Index, Note, Position};
use std::collections::BTreeMap;

pub trait NotesExt {
    fn matches_by<F>(
        self,
        pattern: &[Index],
        song_length: Index,
        f: F,
    ) -> (BTreeMap<Position, Note>, BTreeMap<Position, Note>)
    where
        F: Fn(&Note, &Note) -> bool;

    /// Reassign layers across multiple note groups so they don't overlap.
    fn reassign_layers<I, J>(slices: I) -> BTreeMap<Position, Note>
    where
        I: IntoIterator<Item = J>,
        J: IntoIterator<Item = (Index, Note)>;
}

impl NotesExt for BTreeMap<Position, Note> {
    /// Separates notes into matching and non-matching groups based on pattern matching.
    fn matches_by<F>(
        self,
        pattern: &[Index],
        song_length: Index,
        f: F,
    ) -> (BTreeMap<Position, Note>, BTreeMap<Position, Note>)
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

            // 按偏移模式检查匹配
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

            // 在匹配组成立时选中
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
        (matched, unmatched)
    }

    /// Reassign layers across multiple note groups so they don't overlap.
    fn reassign_layers<I, J>(slices: I) -> BTreeMap<Position, Note>
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

        result
    }
}
