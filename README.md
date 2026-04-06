# rsnbs

> A simple rust library to read and write [.nbs files](https://opennbs.org/nbs) from [Open Note Block Studio](https://opennbs.org/).

This library is a port of `pynbs`, reimplementing its functionality in Rust. However, due to language differences, some adaptations have been made that may lead to behavioral inconsistencies. Additionally, some extra interfaces have been provided based on downstream requirements.

Currently compatible up to version 5. Since this project is quite niche, compatibility with older versions of `.nbs` files hasn't been thoroughly tested. If you encounter any issues or have feature requests, please submit an issue.

## example

iterating over Note Block Studio songs:

```rust
use rsnbs::*;

let song = Song::open_nbs("test_song.nbs").unwrap();
for note in song.notes {
    println!("tick: {}, key: {}", note.tick, note.key)
}
```

or generating new songs programmatically

```rust
use rsnbs::*;

let mut song = Song::new();
song.header.is_loop = true;

for i in 0..25u8 {
    song.notes.push({
        Note::new(i.into(), 0, 0, i + 33)
    })
}
song.save_nbs("test_song.nbs").unwrap();
```
