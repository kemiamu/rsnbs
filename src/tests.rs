use super::*;
use std::collections::BTreeMap;

#[test]
fn test() {
    let mut song = Song::open_nbs("evil_cat_world_ruling_scheme/source.nbs").unwrap();
    let mut notes = Vec::from(song.notes);
    notes.sort_by_key(|n| (n.tick, n.tone()));

    let patterns = PATTERNS;
    // let song_length: Index = 144;
    let song_length: Index = notes.iter().map(|n| n.tick).max().unwrap() + 1;

    let mut slices: Vec<Vec<Note>> = Default::default();
    for &pattern in patterns {
        let (matched, unmatched) =
            notes.matches_by(pattern, song_length, |a, b| a.tone() == b.tone());

        slices.push(matched.clone());
        notes = unmatched;
        // notes.append(&mut matched);
        // notes.sort();
    }

    song.notes = reassign_layers(slices).into();
    song.header.is_loop = true;
    song.save_nbs("evil_cat_world_ruling_scheme/out.nbs")
        .unwrap();
}

#[test]
fn analyze_tones() {
    let mut song = Song::open_nbs("evil_cat_world_ruling_scheme/source.nbs").unwrap();
    let mut notes = Vec::from(song.notes);
    notes.sort_by_key(|n| (n.tick, n.tone()));
    let mut by_tone: BTreeMap<_, Vec<Note>> = BTreeMap::new();
    for note in &notes {
        by_tone.entry(note.tone()).or_default().push(note.clone());
    }
    let slices: Vec<Vec<Note>> = by_tone.into_values().collect();

    song.notes = reassign_layers(slices).into();
    song.header.is_loop = true;
    song.save_nbs("evil_cat_world_ruling_scheme/analyzed.nbs")
        .unwrap();
}

// 按照列表重新分配层级
fn reassign_layers(slices: Vec<Vec<Note>>) -> Vec<Note> {
    let mut base_layer: Index = Default::default();
    let mut result: Vec<Note> = Default::default();

    for notes in slices {
        let mut current_layer: Index = Default::default();
        let mut max_layer: Index = Default::default();

        let mut notes = notes.into_iter().peekable();
        while let Some(mut note) = notes.next() {
            note.layer = base_layer + current_layer;

            max_layer = max_layer.max(current_layer + 2);
            match notes.peek().map(|n| n.tick) == Some(note.tick) {
                true => current_layer += 1,
                false => current_layer = 0,
            }

            result.push(note);
        }
        base_layer += max_layer;
    }

    result
}

#[test]
fn generating_and_load() {
    let mut song = Song::new();
    song.header.is_loop = true;
    for i in 0..25 {
        song.notes.insert((i, 0, 0, i + 33).try_into().unwrap());
    }
    song.save_nbs("evil_cat_world_ruling_scheme/test_song.nbs")
        .unwrap();

    let song = Song::open_nbs("evil_cat_world_ruling_scheme/test_song.nbs").unwrap();
    for note in song.notes {
        println!("tick: {}, key: {}", note.tick, note.key)
    }
}
