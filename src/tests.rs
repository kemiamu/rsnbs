use super::*;
use std::collections::BTreeMap;

#[test]
fn test_pattern_matching() {
    let mut song = Song::open_nbs("evil_cat_world_ruling_scheme/source.nbs").unwrap();
    let mut notes = song.notes;

    let patterns = PATTERNS;
    // let song_length: Index = 144;
    let song_length: Index = notes.iter().map(|(p, _)| p.tick()).max().unwrap() + 1;

    let mut clusters: Vec<BTreeMap<Position, Note>> = Default::default();
    for &pattern in patterns {
        let (matched, unmatched) =
            notes.matches_by(pattern, song_length, |a, b| a.tone() == b.tone());

        clusters.push(matched.clone());
        notes = unmatched;
        // notes.append(&mut matched);
        // notes.sort();
    }

    song.notes = reassign_layers(clusters).into();
    song.header.is_loop = true;
    song.save_nbs("evil_cat_world_ruling_scheme/out.nbs")
        .unwrap();
}

#[test]
fn analyze_tones() {
    let mut song = Song::open_nbs("evil_cat_world_ruling_scheme/source.nbs").unwrap();

    let mut by_tone: BTreeMap<_, Vec<(Position, Note)>> = BTreeMap::new();
    for (pos, note) in song.notes {
        by_tone.entry(note.tone()).or_default().push((pos, note));
    }
    let slices: Vec<BTreeMap<Position, Note>> = by_tone
        .into_values()
        .map(|v| v.into_iter().collect())
        .collect();

    song.notes = reassign_layers(slices);
    song.header.is_loop = true;
    song.save_nbs("evil_cat_world_ruling_scheme/analyzed.nbs")
        .unwrap();
}

// 按照列表重新分配层级
fn reassign_layers(slices: Vec<BTreeMap<Position, Note>>) -> BTreeMap<Position, Note> {
    let mut base_layer: Index = Default::default();
    let mut result: BTreeMap<Position, Note> = Default::default();

    for notes in slices {
        let mut current_layer: Index = Default::default();
        let mut max_layer: Index = Default::default();

        let mut notes = notes.into_iter().peekable();
        while let Some((mut pos, note)) = notes.next() {
            pos.layer = base_layer + current_layer;

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

#[test]
fn generating_and_load() {
    let song_path = "evil_cat_world_ruling_scheme/test_song.nbs";

    // README.md Example: Generating and loading a song
    let mut song = Song::new();
    song.header.is_loop = true;
    for i in 0..25 {
        let pos = Position::new(i, 0);
        let note = Note::new(Instrument::Harp, Key::from_minecraft_note(i).unwrap());
        song.notes.insert(pos, note);
    }
    song.save_nbs(song_path).unwrap();

    // README.md Example: Iterating over notes
    let song = Song::open_nbs(song_path).unwrap();
    for (position, note) in song.notes {
        println!("tick: {}, key: {}", position.tick, note.key)
    }
}
