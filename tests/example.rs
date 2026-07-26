// README examples
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Corresponds to the two code blocks in README.md.
#[test]
fn test_readme_example() {
    use rsnbs::note::{Instrument, Key, Note, Tone};
    use rsnbs::song::Song;
    use rsnbs::types::{IntoTick, Position, Tick};

    // example 1: iterating over a song's notes
    let song: Song = Song::open_nbs("fixtures/source.nbs").unwrap();
    for (pos, note) in &song.notes {
        let tick: Tick = pos.into_tick();
        let key: Key = note.tone().key();
        println!("tick: {tick}, key: {key}");
    }

    // example 2: generating a new song programmatically
    let mut song: Song = Song::new();
    song.header.is_loop = true;
    for i in 0..25 {
        let pos: Position = Position::new(i, 0);
        let key: Key = Key::from_minecraft_note(i).unwrap();
        let tone: Tone = Tone::new(Instrument::Harp, key);
        song.notes.insert(pos, Note::new(tone));
    }
    song.save_nbs("fixtures/generated_example.nbs").unwrap();
}
