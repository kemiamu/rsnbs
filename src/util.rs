use crate::{Index, Note, Song};

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
        match self.notes.iter().max_by_key(|n| n.tick) {
            Some(last_note) => self.header.song_length = last_note.tick,
            None => self.header.song_length = 1,
        }
        // 更新 layer 数量
        self.header.song_layers = self.layers.len() as _;
        // 更新 notes
        // self.notes.refresh();
    }
}

pub trait NotesExt {
    fn matches_by<F>(self, pattern: Vec<Index>, song_length: Index, f: F) -> (Vec<Note>, Vec<Note>)
    where
        F: Fn(&Note, &Note) -> bool;
}

impl<T> NotesExt for T
where
    T: IntoIterator<Item = Note>,
    for<'a> &'a T: IntoIterator<Item = &'a Note>,
{
    /// Separates notes into matching and non-matching groups based on pattern matching.
    fn matches_by<F>(self, pattern: Vec<Index>, song_length: Index, f: F) -> (Vec<Note>, Vec<Note>)
    where
        F: Fn(&Note, &Note) -> bool,
    {
        let mut notes: Vec<(Note, bool)> = self.into_iter().map(|n| (n, false)).collect();

        for i in 0..notes.len() {
            if notes[i].1 {
                continue;
            }

            // 按偏移模式检查匹配
            let base = notes[i].0.tick;
            let result = pattern.iter().try_fold(vec![], |mut indices, &p| {
                let target = (base + p) % song_length;
                notes
                    .iter()
                    .enumerate()
                    .find(|(_, (note, is_matched))| {
                        !is_matched && note.tick == target && f(note, &notes[i].0)
                    })
                    .map(|(idx, _)| {
                        indices.push(idx);
                        indices
                    })
            });

            // 匹配组成立时选中
            if let Some(indices) = result {
                for &idx in &indices {
                    notes[idx].1 = true;
                }
            }
        }

        let (mut matched, mut unmatched) = (Vec::new(), Vec::new());
        for (note, is_matched) in notes {
            match is_matched {
                true => matched.push(note),
                false => unmatched.push(note),
            }
        }
        (matched, unmatched)
    }
}
