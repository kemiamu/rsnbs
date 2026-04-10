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
    fn is_cyclic<F, K>(&self, pred: &Note, length: Index, by_key: F) -> Index
    where
        F: Fn(&Note) -> K,
        K: Eq;

    fn cyclic_matches<F, K>(self, len: Index, pow: Index, by_key: F) -> (Vec<Note>, Vec<Note>)
    where
        F: Fn(&Note) -> K,
        K: Eq;
}

impl<T> NotesExt for T
where
    T: IntoIterator<Item = Note>,
    for<'a> &'a T: IntoIterator<Item = &'a Note>,
{
    /// Returns true if any note other than `pred` shares the same key in the cycle.
    fn is_cyclic<F, K>(&self, pred: &Note, len: Index, by_key: F) -> Index
    where
        F: Fn(&Note) -> K,
        K: Eq,
    {
        let key = (pred.tick % len, by_key(pred));
        let (_, count) = self.into_iter().fold((None, 0), |(tick, count), n| {
            match Some(n.tick) != tick && (n.tick % len, by_key(n)) == key {
                true => (Some(n.tick), count + 1),
                false => (tick, count),
            }
        });
        count
    }

    /// Returns notes that are cyclic and orphan notes in the cycle.
    fn cyclic_matches<F, K>(self, len: Index, pow: Index, by_key: F) -> (Vec<Note>, Vec<Note>)
    where
        F: Fn(&Note) -> K,
        K: Eq,
    {
        let mut matches = vec![];
        let mut orphan: Vec<Note> = self.into_iter().collect();
        loop {
            let match_flags: Vec<usize> = orphan
                .iter()
                .enumerate()
                .filter_map(|(i, note)| (orphan.is_cyclic(note, len, &by_key) >= pow).then_some(i))
                .collect();
            if match_flags.is_empty() {
                return (matches, orphan);
            }
            for flag in match_flags.into_iter().rev() {
                matches.push(orphan.remove(flag));
            }
        }
    }
}

// pub trait SaturatingCast<T> {
//     fn saturating_into(self) -> T;
// }

// macro_rules! impl_saturating_cast {
//     ($from:ty => $to:ty) => {
//         impl SaturatingCast<$to> for $from {
//             fn saturating_into(self) -> $to {
//                 self.try_into().unwrap_or(<$to>::MAX)
//             }
//         }
//     };
// }

// impl_saturating_cast!(u32 => u8);
// impl_saturating_cast!(u32 => u16);
// impl_saturating_cast!(usize => u8);
// impl_saturating_cast!(usize => u32);
