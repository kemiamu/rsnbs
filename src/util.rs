use crate::{Index, Note, Notes, Song};

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

impl Notes {
    /// Finds notes matching `pred` in cycle and `f`-value, deduped by tick.
    ///
    /// **Assumes:** notes sorted by tick.
    pub fn cyclic_at<F, T>(&self, pred: &Note, length: Index, f: F) -> Vec<&Note>
    where
        F: Fn(&Note) -> T,
        T: Eq,
    {
        let key = (pred.tick % length, f(pred));
        let mut matches: Vec<_> = self
            .iter()
            .filter(|&n| (n.tick % length, f(n)) == key)
            .collect();
        // matches.sort();
        matches.dedup_by_key(|n| n.tick);
        matches
    }

    /// Finds the first note that has cyclic matches and returns them.
    ///
    /// Iterates through all notes in `self`, calling `cyclic_at` for each note.
    /// Returns `Some(matches)` for the first note that has cyclic matches,
    /// or `None` if no note has any cyclic matches.
    pub fn cyclic_matches<F, T>(&self, length: Index, f: F) -> Option<Vec<&Note>>
    where
        F: Fn(&Note) -> T + Copy,
        T: Eq,
    {
        self.iter().find_map(|note| {
            let matches = self.cyclic_at(note, length, f);
            matches.len().gt(&1).then_some(matches)
        })
    }
}
