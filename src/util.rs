use std::{collections::HashMap, hash::Hash, vec};

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
    // fn count_cycle<F, K>(&self, pred: &Note, length: Index, by_key: F) -> Index
    // where
    //     F: Fn(&Note) -> K,
    //     K: Eq;

    fn cyclic_matches<F, K>(self, len: Index, pow: Index, by_key: F) -> (Vec<Note>, Vec<Note>)
    where
        F: Fn(&Note) -> K,
        K: Eq + Ord + Hash;
}

impl<T> NotesExt for T
where
    T: IntoIterator<Item = Note>,
    for<'a> &'a T: IntoIterator<Item = &'a Note>,
{
    // /// Counts how many notes share the same cyclic pattern as the predicate note.
    // fn count_cycle<F, K>(&self, pred: &Note, len: Index, by_key: F) -> Index
    // where
    //     F: Fn(&Note) -> K,
    //     K: Eq,
    // {
    //     let notes: BTreeSet<&Note> = self.into_iter().collect();
    //     let key = (pred.tick % len, by_key(pred));
    //     // 按循环特征构建层级权重
    //     let (_, count) = notes.into_iter().fold((None, 0), |(tick, count), n| {
    //         match Some(n.tick) != tick && (n.tick % len, by_key(n)) == key {
    //             true => (Some(n.tick), count + 1),
    //             false => (tick, count),
    //         }
    //     });
    //     count
    // }

    /// Separates notes into matching and non-matching groups based on cyclic patterns.
    fn cyclic_matches<F, K>(self, len: Index, freq: Index, by_key: F) -> (Vec<Note>, Vec<Note>)
    where
        F: Fn(&Note) -> K,
        K: Eq + Ord + Hash,
    {
        // (note, index)
        let notes_with_index: Vec<(Note, Index)> = {
            let mut notes: Vec<Note> = self.into_iter().collect();
            notes.sort_by_key(|n| (n.tick, by_key(n)));

            let mut result: Vec<(Note, Index)> = vec![];
            let mut prev_key: Option<(Index, K)> = None;
            let mut index = Index::default();

            for note in notes {
                let key = (note.tick, by_key(&note));
                match Some(&key) == prev_key.as_ref() {
                    true => index += 1,
                    false => {
                        index = Index::default();
                        prev_key = Some(key)
                    }
                }
                result.push((note, index));
            }
            result
        };

        // (note, freq)
        let notes_freq: Vec<(Note, Index)> = {
            let make_key = |note: &Note, index: Index| (index, note.tick % len, by_key(note));
            let mut freq = HashMap::new();
            for (note, index) in &notes_with_index {
                *freq
                    .entry(make_key(note, *index))
                    .or_insert(Index::default()) += 1;
            }

            notes_with_index
                .into_iter()
                .map(|(note, index)| {
                    let key = make_key(&note, index);
                    (note, freq[&key])
                })
                .collect()
        };

        // if freq >= pow
        let (matches, orphan) = {
            let mut matches = Vec::new();
            let mut orphan = Vec::new();

            for (note, note_freq) in notes_freq {
                match note_freq >= freq {
                    true => matches.push(note),
                    false => orphan.push(note),
                }
            }
            (matches, orphan)
        };

        (matches, orphan)
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
