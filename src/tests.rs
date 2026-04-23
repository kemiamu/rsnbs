use super::*;

#[test]
fn test() {
    let mut song = Song::open_nbs("evil_cat_world_ruling_scheme/source.nbs").unwrap();
    let mut notes = Vec::from(song.notes);

    let patterns: Vec<Vec<Index>> = vec![
        vec![0, 3, 6, 72, 75, 78],
        // vec![0, 3, 6],
        // vec![0, 18, 54, 90, 126, 36, 108, 72],
        // vec![0, 36, 108, 72],
        vec![0, 72],
        vec![0],
    ];
    // let song_length: Index = 144;
    let song_length: Index = notes.iter().map(|n| n.tick).max().unwrap() + 1;

    let mut slices: Vec<Vec<Note>> = Default::default();
    for pattern in patterns {
        let (matched, unmatched) =
            notes.matches_by(pattern, song_length, |a, b| a.tone() == b.tone());

        slices.push(matched.clone());
        notes = unmatched;
        // notes.append(&mut matched);
        // notes.sort();
    }

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

    song.notes = result.into();
    song.header.is_loop = true;
    song.save_nbs("evil_cat_world_ruling_scheme/out.nbs")
        .unwrap();
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
