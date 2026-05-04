use std::collections::BTreeMap;

use crate::{Index, Note, Position, Song};

/// A trait for types that can be refreshed to ensure data consistency.
pub trait Refreshable {
    /// Refreshes the data to ensure consistency.
    ///
    /// This method should update the internal state of the type
    /// to reflect the most current and consistent data.
    fn refresh(&mut self);
}

impl Refreshable for Song {
    fn refresh(&mut self) {
        // 从音符计算歌曲长度
        match self.notes.iter().map(|(pos, _)| pos.tick).max() {
            Some(last_tick) => self.header.song_length = last_tick,
            None => self.header.song_length = 1,
        }
        // 更新 layer 数量
        self.header.song_layers = self.layers.len() as _;
        // 更新 notes
        // self.notes.refresh();
    }
}

pub trait NotesExt {
    fn matches_by<F>(
        self,
        pattern: &[Index],
        song_length: Index,
        f: F,
    ) -> (BTreeMap<Position, Note>, BTreeMap<Position, Note>)
    where
        F: Fn(&Note, &Note) -> bool;
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
}
