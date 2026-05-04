# rsnbs

> A simple rust library to read and write [.nbs files](https://opennbs.org/nbs) from [Open Note Block Studio](https://opennbs.org/).

This library is a port of [pynbs](https://github.com/OpenNBS/pynbs), reimplementing its functionality in Rust. However, due to language differences, some adaptations have been made that may lead to behavioral inconsistencies. Additionally, some extra interfaces have been provided based on downstream requirements.

Currently compatible up to version 5. Since this project is quite niche, compatibility with older versions of `.nbs` files hasn't been thoroughly tested. If you encounter any issues or have feature requests, please submit an issue.

## example

iterating over Note Block Studio songs:

```rust
use rsnbs::*;
let song_path = "test_song.nbs";

let song = Song::open_nbs(song_path).unwrap();
for (position, note) in song.notes {
    println!("tick: {}, key: {}", position.tick, note.key)
}
```

or generating new songs programmatically

```rust
use rsnbs::*;
let song_path = "test_song.nbs";

let mut song = Song::new();
song.header.is_loop = true;
for i in 0..25 {
    let pos = Position::new(i, 0);
    let note = Note::new(Instrument::Harp, Key::from_minecraft_note(i).unwrap());
    song.notes.insert(pos, note);
}
song.save_nbs(song_path).unwrap();
```
