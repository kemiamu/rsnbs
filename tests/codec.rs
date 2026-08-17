//! Codec tests with self-contained byte fixtures (no external .nbs files).
//!
//! The `build` helper writes NBS files byte-by-byte per the OpenNBS spec,
//! so these tests are independent of the repo's gitignored fixtures.

use rsnbs::note::Instrument;
use rsnbs::song::Song;
use rsnbs::types::{IntoTick, Version};
use std::io::Cursor;

fn write_str(buf: &mut Vec<u8>, s: &str) {
    let b = s.as_bytes();
    buf.extend((b.len() as u32).to_le_bytes());
    buf.extend(b);
}

fn push_note(buf: &mut Vec<u8>, version: u8, layer_jump: u16, inst: u8, key: u8) {
    buf.extend(layer_jump.to_le_bytes());
    buf.push(inst);
    buf.push(key);
    if version >= 4 {
        buf.push(100); // velocity
        buf.push(100); // panning
        buf.extend(0i16.to_le_bytes()); // pitch
    }
}

#[allow(clippy::too_many_arguments)]
fn build(
    version: u8,
    fci: u8,
    song_len: u16,
    layers: u16,
    notes: &[(Option<u16>, u16, u8, u8)], // (tick_jump, layer_jump, instrument byte, key)
    layer_names: &[(&str, u8, u8, u8)],   // (name, lock, volume, panning)
    customs: &[(&str, &str, u8, u8)],     // (name, file, key, press_key)
) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut first_note = true;
    if version > 0 {
        buf.extend(0u16.to_le_bytes()); // new format marker
        buf.push(version);
        buf.push(fci);
    } else {
        buf.extend(song_len.to_le_bytes());
    }
    if version >= 3 {
        buf.extend(song_len.to_le_bytes());
    }
    buf.extend(layers.to_le_bytes());
    write_str(&mut buf, "Test Song");
    write_str(&mut buf, "kemiamu");
    write_str(&mut buf, "");
    write_str(&mut buf, "");
    buf.extend(1000u16.to_le_bytes()); // tempo
    buf.push(0); // autosave
    buf.push(10); // autosave minutes
    buf.push(4); // time signature
    for _ in 0..5 {
        buf.extend(0i32.to_le_bytes());
    }
    write_str(&mut buf, ""); // midi
    if version >= 4 {
        buf.push(0); // loop
        buf.push(0); // max loop count
        buf.extend(0u16.to_le_bytes()); // loop start
    }
    for &(tj, lj, inst, key) in notes {
        if let Some(tj) = tj {
            if !first_note {
                buf.extend(0u16.to_le_bytes()); // end of the previous tick's layers
            }
            buf.extend(tj.to_le_bytes());
        }
        push_note(&mut buf, version, lj, inst, key);
        first_note = false;
    }
    buf.extend(0u16.to_le_bytes()); // end of the last tick's layers
    buf.extend(0u16.to_le_bytes()); // end of notes
    for &(name, lock, vol, pan) in layer_names {
        write_str(&mut buf, name);
        if version >= 4 {
            buf.push(lock);
        }
        buf.push(vol);
        if version >= 2 {
            buf.push(pan);
        }
    }
    buf.push(customs.len() as u8);
    for &(name, file, key, press) in customs {
        write_str(&mut buf, name);
        write_str(&mut buf, file);
        buf.push(key);
        buf.push(press);
    }
    buf
}

fn parse(bytes: Vec<u8>) -> Song {
    let mut cursor = Cursor::new(bytes);
    Song::parse(&mut cursor).unwrap()
}

fn write(song: &mut Song) -> Vec<u8> {
    let mut buf = Vec::new();
    song.write(&mut buf).unwrap();
    buf
}

// A v6 file: trumpets are vanilla (fci = 20), bytes at or above 20 are custom.
#[test]
fn v6_parses_trumpets_as_vanilla_and_bytes_as_custom_slots() {
    let bytes = build(
        6,
        20,
        0,
        1,
        &[
            (Some(1), 1, 16, 45), // tick 0, layer 0: byte 16 -> Trumpet (vanilla in v6)
            (None, 2, 21, 45),    // tick 0, layer 2: byte 21 -> custom slot 1
        ],
        &[("Layer", 0, 100, 100)],
        &[("A", "a.ogg", 45, 1), ("B", "b.ogg", 45, 1)],
    );
    let song = parse(bytes);
    assert_eq!(song.header.version.get(), 6);
    assert_eq!(song.header.default_instruments, 20);

    let notes: Vec<_> = song.notes.iter().collect();
    assert_eq!(notes.len(), 2);
    assert_eq!(notes[0].1.tone().instrument(), Instrument::Trumpet);
    assert_eq!(notes[1].1.tone().instrument(), Instrument::Custom(1));
    assert_eq!(song.custom_instruments[1].name, "B");
}

// Regression: a v5 custom instrument at byte 17 must not be misread as a trumpet.
#[test]
fn v5_parses_custom_byte_as_custom_slot_not_trumpet() {
    let bytes = build(
        5,
        16,
        0,
        1,
        &[(Some(1), 1, 17, 45)], // byte 17 >= fci 16 -> custom slot 1
        &[("Layer", 0, 100, 100)],
        &[("A", "a.ogg", 45, 1), ("B", "b.ogg", 45, 1)],
    );
    let song = parse(bytes);
    assert_eq!(song.header.default_instruments, 16);
    let notes: Vec<_> = song.notes.iter().collect();
    assert_eq!(notes[0].1.tone().instrument(), Instrument::Custom(1));
}

// Downgrading a v6 song with trumpets to v5 must convert them to custom
// instruments (following OpenNBS naming) instead of writing broken bytes.
#[test]
fn v6_to_v5_downgrade_converts_trumpets_to_custom_instruments() {
    let bytes = build(
        6,
        20,
        0,
        1,
        &[
            (Some(1), 1, 16, 45), // Trumpet
            (None, 1, 20, 45),    // custom slot 0: "Custom Brass"
        ],
        &[("Layer", 0, 100, 100)],
        &[("Custom Brass", "brass.ogg", 45, 1)],
    );
    let mut song = parse(bytes);
    song.header.version = Version::new(5).unwrap();

    let out = write(&mut song);
    let downgraded = parse(out.clone());

    assert_eq!(downgraded.header.version.get(), 5);
    assert_eq!(downgraded.header.default_instruments, 16);
    let notes: Vec<_> = downgraded.notes.iter().collect();
    assert_eq!(notes[0].1.tone().instrument(), Instrument::Custom(1));
    assert_eq!(notes[1].1.tone().instrument(), Instrument::Custom(0));
    assert_eq!(downgraded.custom_instruments[1].name, "Trumpet");
    assert_eq!(downgraded.custom_instruments[1].file, "trumpet.ogg");
    assert_eq!(downgraded.custom_instruments[0].name, "Custom Brass");
}

// Downgrading without trumpets must still remap custom instrument slots,
// since v6 files use fci = 20 while v5 files use fci = 16.
#[test]
fn v6_to_v5_downgrade_remaps_custom_slots() {
    let bytes = build(
        6,
        20,
        0,
        1,
        &[(Some(1), 1, 21, 45)], // custom slot 1: "B"
        &[("Layer", 0, 100, 100)],
        &[("A", "a.ogg", 45, 1), ("B", "b.ogg", 45, 1)],
    );
    let mut song = parse(bytes);
    song.header.version = Version::new(5).unwrap();

    let out = write(&mut song);
    assert_eq!(out[3], 16, "v5 file must claim 16 vanilla instruments");

    let downgraded = parse(out);
    let notes: Vec<_> = downgraded.notes.iter().collect();
    assert_eq!(notes[0].1.tone().instrument(), Instrument::Custom(1));
    assert_eq!(downgraded.custom_instruments[1].name, "B");
}

// Upgrading a v5 song to v6 must shift custom instrument bytes from 16 + slot
// to 20 + slot.
#[test]
fn v5_to_v6_upgrade_remaps_custom_slots() {
    let bytes = build(
        5,
        16,
        0,
        1,
        &[(Some(1), 1, 17, 45)], // custom slot 1: "B"
        &[("Layer", 0, 100, 100)],
        &[("A", "a.ogg", 45, 1), ("B", "b.ogg", 45, 1)],
    );
    let mut song = parse(bytes);
    song.header.version = Version::new(6).unwrap();

    let out = write(&mut song);
    assert_eq!(out[3], 20, "v6 file must claim 20 vanilla instruments");

    let upgraded = parse(out);
    let notes: Vec<_> = upgraded.notes.iter().collect();
    assert_eq!(notes[0].1.tone().instrument(), Instrument::Custom(1));
    assert_eq!(upgraded.custom_instruments[1].name, "B");
}

#[test]
fn new_song_defaults_to_v6_with_20_vanilla_instruments() {
    let song = Song::new();
    assert_eq!(song.header.version.get(), 6);
    assert_eq!(song.header.default_instruments, 20);
}

#[test]
fn v6_roundtrip_is_byte_identical() {
    let bytes = build(
        6,
        20,
        0,
        1,
        &[
            (Some(1), 1, 18, 45), // Trumpet
            (None, 2, 20, 45),    // custom slot 0
        ],
        &[("Layer", 0, 100, 100)],
        &[("Custom", "custom.ogg", 45, 1)],
    );
    let mut song = parse(bytes.clone());
    assert_eq!(write(&mut song), bytes);
}

#[test]
fn v5_roundtrip_is_byte_identical() {
    let bytes = build(
        5,
        16,
        0,
        1,
        &[(Some(1), 1, 16, 45)], // custom slot 0
        &[("Layer", 0, 100, 100)],
        &[("Custom", "custom.ogg", 45, 1)],
    );
    let mut song = parse(bytes.clone());
    assert_eq!(write(&mut song), bytes);
}

#[test]
fn v1_roundtrip_is_byte_identical() {
    let bytes = build(
        1,
        16,
        0,
        1,
        &[(Some(1), 1, 3, 45)], // no velocity/panning/pitch in v1
        &[("Layer", 0, 100, 100)],
        &[("Custom", "custom.ogg", 45, 1)],
    );
    let mut song = parse(bytes.clone());
    assert_eq!(song.header.version.get(), 1);
    assert_eq!(write(&mut song), bytes);
}

#[test]
fn classic_v0_parses_and_roundtrips() {
    // The first u16 is the song length in the classic format and must stay
    // consistent with the last note's tick when refresh() recomputes it.
    let bytes = build(
        0,
        0,
        4,
        1,
        &[(Some(5), 1, 3, 45)], // tick 4: snare drum, no velocity/panning/pitch
        &[("Layer", 0, 100, 100)],
        &[("Custom", "custom.ogg", 45, 1)],
    );
    let mut song = parse(bytes.clone());
    assert_eq!(song.header.version.get(), 0);
    assert_eq!(song.header.default_instruments, 10);
    let notes: Vec<_> = song.notes.iter().collect();
    assert_eq!(notes[0].1.tone().instrument(), Instrument::SnareDrum);
    assert_eq!(write(&mut song), bytes);
}

// The fallback is generic: any vanilla instrument that does not exist in the
// target version (not just the trumpets) becomes a custom instrument.
#[test]
fn generic_fallback_covers_all_unsupported_vanilla_instruments() {
    let bytes = build(
        6,
        20,
        0,
        1,
        &[
            (Some(5), 1, 2, 45), // BassDrum: index 2 < 10, stays vanilla in v0
            (None, 1, 10, 45),   // Iron Xylophone: index 10 >= 10 -> custom
            (None, 1, 14, 45),   // Banjo: index 14 >= 10 -> custom
            (None, 1, 16, 45),   // Trumpet: index 16 >= 10 -> custom
        ],
        &[("Layer", 0, 100, 100)],
        &[],
    );
    let mut song = parse(bytes);
    song.header.version = Version::new(0).unwrap();

    let out = write(&mut song);
    let downgraded = parse(out);
    assert_eq!(downgraded.header.default_instruments, 10);
    let definitions: Vec<(&str, &str)> = downgraded
        .custom_instruments
        .iter()
        .map(|ci| (ci.name.as_str(), ci.file.as_str()))
        .collect();
    assert_eq!(
        definitions,
        [
            ("Iron Xylophone", "iron_xylophone.ogg"),
            ("Banjo", "banjo.ogg"),
            ("Trumpet", "trumpet.ogg"),
        ]
    );
    let instruments: Vec<Instrument> = downgraded
        .notes
        .values()
        .map(|note| note.tone().instrument())
        .collect();
    assert_eq!(
        instruments,
        [
            Instrument::BassDrum,
            Instrument::Custom(0),
            Instrument::Custom(1),
            Instrument::Custom(2),
        ]
    );
}

// The trumpets downgrade must deduplicate identical conversions and keep the
// note positions intact.
#[test]
fn v6_to_v5_downgrade_deduplicates_trumpets() {
    let bytes = build(
        6,
        20,
        0,
        2,
        &[
            (Some(1), 1, 19, 45), // Oxidized Trumpet
            (Some(1), 1, 19, 50), // another Oxidized Trumpet
            (Some(1), 1, 17, 55), // Exposed Trumpet
        ],
        &[("L1", 0, 100, 100), ("L2", 0, 100, 100)],
        &[],
    );
    let mut song = parse(bytes);
    song.header.version = Version::new(5).unwrap();
    let out = write(&mut song);

    let downgraded = parse(out);
    assert_eq!(downgraded.custom_instruments.len(), 2);
    let names: Vec<&str> = downgraded
        .custom_instruments
        .iter()
        .map(|ci| ci.name.as_str())
        .collect();
    assert_eq!(names, ["Oxidized Trumpet", "Exposed Trumpet"]);
    let notes: Vec<_> = downgraded.notes.iter().collect();
    assert_eq!(notes.len(), 3);
    let ticks: Vec<_> = notes.iter().map(|(p, _)| p.into_tick()).collect();
    assert_eq!(ticks, [0, 1, 2]);
}
