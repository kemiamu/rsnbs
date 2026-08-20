//! NBS (Note Block Studio) file format parser and writer.

use crate::note::{Instrument, Key, Note, Notes};
use crate::song::{CustomInstrument, Header, Layer, Song};
use crate::types::{Panning, Position, Result, Version, Volume};
use std::borrow::Cow;
use std::io;

mod impls;
mod nbs_ext;

type CowHeader<'a> = Cow<'a, Header>;
type CowNotes<'a> = Cow<'a, Notes<Position, Note>>;
type CowNote<'a> = Cow<'a, Note>;
type CowLayer<'a> = Cow<'a, Layer>;
type CowCustomInst<'a> = Cow<'a, CustomInstrument>;
type CowCustomInsts<'a> = Cow<'a, [CustomInstrument]>;
type CowSong<'a> = Cow<'a, Song>;

// Parse/Write
//
// ++++++++++++============++++++++++++============++++++++++++============

/// unified trait for both parsing and writing data, optionally with context
pub(super) trait Codec {
    /// context type shared for both parsing and writing (use () when no context is needed)
    type Context: Copy;

    /// the type parse produces; usually Self, encoding wrappers override it with the wrapped type
    type Target;

    /// parse data from a reader with context
    fn parse<R: io::Read, M: Middleware + ?Sized>(
        reader: &mut R,
        context: Self::Context,
        middlewares: &mut M,
    ) -> Result<Self::Target>;

    /// write data to a writer with context
    fn write<W: io::Write, M: Middleware + ?Sized>(
        &self,
        writer: &mut W,
        context: Self::Context,
        middlewares: &mut M,
    ) -> Result<()>;
}

// Middleware
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Per-level transform hooks; parse calls decode_*, write calls encode_*.
pub(super) trait Middleware {
    fn decode_header(&mut self, header: Header) -> Header {
        header
    }
    fn encode_header<'a>(&mut self, header: CowHeader<'a>) -> CowHeader<'a> {
        header
    }

    fn decode_song(&mut self, song: Song) -> Song {
        song
    }
    fn encode_song<'a>(&mut self, song: CowSong<'a>) -> CowSong<'a> {
        song
    }

    fn decode_custom_insts(&mut self, customs: Vec<CustomInstrument>) -> Vec<CustomInstrument> {
        customs
    }
    fn encode_custom_insts<'a>(&mut self, customs: CowCustomInsts<'a>) -> CowCustomInsts<'a> {
        customs
    }

    fn decode_notes(&mut self, notes: Notes<Position, Note>) -> Notes<Position, Note> {
        notes
    }
    fn encode_notes<'a>(&mut self, notes: CowNotes<'a>) -> CowNotes<'a> {
        notes
    }

    fn decode_note(&mut self, note: Note) -> Note {
        note
    }
    fn encode_note<'a>(&mut self, note: CowNote<'a>) -> CowNote<'a> {
        note
    }

    fn decode_layer(&mut self, layer: Layer) -> Layer {
        layer
    }
    fn encode_layer<'a>(&mut self, layer: CowLayer<'a>) -> CowLayer<'a> {
        layer
    }

    fn decode_custom_inst(&mut self, instrument: CustomInstrument) -> CustomInstrument {
        instrument
    }
    fn encode_custom_inst<'a>(&mut self, instrument: CowCustomInst<'a>) -> CowCustomInst<'a> {
        instrument
    }

    fn decode_version(&mut self, version: Version) -> Version {
        version
    }
    fn encode_version(&mut self, version: Version) -> Version {
        version
    }

    fn decode_instrument(&mut self, instrument: Instrument) -> Instrument {
        instrument
    }
    fn encode_instrument(&mut self, instrument: Instrument) -> Instrument {
        instrument
    }

    fn decode_volume(&mut self, volume: Volume) -> Volume {
        volume
    }
    fn encode_volume(&mut self, volume: Volume) -> Volume {
        volume
    }

    fn decode_key(&mut self, key: Key) -> Key {
        key
    }
    fn encode_key(&mut self, key: Key) -> Key {
        key
    }

    fn decode_panning(&mut self, panning: Panning) -> Panning {
        panning
    }
    fn encode_panning(&mut self, panning: Panning) -> Panning {
        panning
    }

    fn decode_f32(&mut self, value: f32) -> f32 {
        value
    }
    fn encode_f32(&mut self, value: f32) -> f32 {
        value
    }
}

/// Identity chain tail.
impl Middleware for () {}

/// Tuple combinator: chains two middlewares, .0 runs first.
impl<A: Middleware, B: Middleware> Middleware for (A, B) {
    fn decode_header(&mut self, header: Header) -> Header {
        let header = self.0.decode_header(header);
        self.1.decode_header(header)
    }
    fn encode_header<'a>(&mut self, header: CowHeader<'a>) -> CowHeader<'a> {
        let header = self.0.encode_header(header);
        self.1.encode_header(header)
    }

    fn decode_song(&mut self, song: Song) -> Song {
        let song = self.0.decode_song(song);
        self.1.decode_song(song)
    }
    fn encode_song<'a>(&mut self, song: CowSong<'a>) -> CowSong<'a> {
        let song = self.0.encode_song(song);
        self.1.encode_song(song)
    }

    fn decode_custom_insts(&mut self, customs: Vec<CustomInstrument>) -> Vec<CustomInstrument> {
        let customs = self.0.decode_custom_insts(customs);
        self.1.decode_custom_insts(customs)
    }
    fn encode_custom_insts<'a>(&mut self, customs: CowCustomInsts<'a>) -> CowCustomInsts<'a> {
        let table = self.0.encode_custom_insts(customs);
        self.1.encode_custom_insts(table)
    }

    fn decode_notes(&mut self, notes: Notes<Position, Note>) -> Notes<Position, Note> {
        let notes = self.0.decode_notes(notes);
        self.1.decode_notes(notes)
    }
    fn encode_notes<'a>(&mut self, notes: CowNotes<'a>) -> CowNotes<'a> {
        let notes = self.0.encode_notes(notes);
        self.1.encode_notes(notes)
    }

    fn decode_note(&mut self, note: Note) -> Note {
        let note = self.0.decode_note(note);
        self.1.decode_note(note)
    }
    fn encode_note<'a>(&mut self, note: CowNote<'a>) -> CowNote<'a> {
        let note = self.0.encode_note(note);
        self.1.encode_note(note)
    }

    fn decode_layer(&mut self, layer: Layer) -> Layer {
        let layer = self.0.decode_layer(layer);
        self.1.decode_layer(layer)
    }
    fn encode_layer<'a>(&mut self, layer: CowLayer<'a>) -> CowLayer<'a> {
        let layer = self.0.encode_layer(layer);
        self.1.encode_layer(layer)
    }

    fn decode_custom_inst(&mut self, instrument: CustomInstrument) -> CustomInstrument {
        let instrument = self.0.decode_custom_inst(instrument);
        self.1.decode_custom_inst(instrument)
    }
    fn encode_custom_inst<'a>(&mut self, instrument: CowCustomInst<'a>) -> CowCustomInst<'a> {
        let instrument = self.0.encode_custom_inst(instrument);
        self.1.encode_custom_inst(instrument)
    }

    fn decode_version(&mut self, version: Version) -> Version {
        let version = self.0.decode_version(version);
        self.1.decode_version(version)
    }
    fn encode_version(&mut self, version: Version) -> Version {
        let version = self.0.encode_version(version);
        self.1.encode_version(version)
    }

    fn decode_instrument(&mut self, instrument: Instrument) -> Instrument {
        let instrument = self.0.decode_instrument(instrument);
        self.1.decode_instrument(instrument)
    }
    fn encode_instrument(&mut self, instrument: Instrument) -> Instrument {
        let instrument = self.0.encode_instrument(instrument);
        self.1.encode_instrument(instrument)
    }

    fn decode_volume(&mut self, volume: Volume) -> Volume {
        let volume = self.0.decode_volume(volume);
        self.1.decode_volume(volume)
    }
    fn encode_volume(&mut self, volume: Volume) -> Volume {
        let volume = self.0.encode_volume(volume);
        self.1.encode_volume(volume)
    }

    fn decode_key(&mut self, key: Key) -> Key {
        let key = self.0.decode_key(key);
        self.1.decode_key(key)
    }
    fn encode_key(&mut self, key: Key) -> Key {
        let key = self.0.encode_key(key);
        self.1.encode_key(key)
    }

    fn decode_panning(&mut self, panning: Panning) -> Panning {
        let panning = self.0.decode_panning(panning);
        self.1.decode_panning(panning)
    }
    fn encode_panning(&mut self, panning: Panning) -> Panning {
        let panning = self.0.encode_panning(panning);
        self.1.encode_panning(panning)
    }

    fn decode_f32(&mut self, value: f32) -> f32 {
        let value = self.0.decode_f32(value);
        self.1.decode_f32(value)
    }
    fn encode_f32(&mut self, value: f32) -> f32 {
        let value = self.0.encode_f32(value);
        self.1.encode_f32(value)
    }
}

// Song
//
// ++++++++++++============++++++++++++============++++++++++++============

impl Song {
    /// parses a complete Song from a reader
    pub fn parse<R: io::Read>(reader: &mut R) -> Result<Self> {
        let mut middlewares = InstrumentTranslate::new();
        Self::parse_with(reader, &mut middlewares)
    }

    /// writes the song to a writer.
    pub fn write<W: io::Write>(&self, writer: &mut W) -> Result<()> {
        let mut middlewares = InstrumentTranslate::new();
        self.write_with(writer, &mut middlewares)
    }
}

// InstrumentTranslate
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Folds preset instruments on parse, injects them on write.
pub(super) struct InstrumentTranslate {
    version: Option<Version>,
}

impl InstrumentTranslate {
    pub(super) fn new() -> Self {
        InstrumentTranslate { version: None }
    }

    /// Vanilla instruments beyond the target version's FCI.
    fn presets(&self) -> impl Iterator<Item = Instrument> {
        let fci = self.version.unwrap().vanilla_instruments() as usize;
        Instrument::NBS_INDEX.into_iter().skip(fci)
    }

    /// Preset definitions written at the head of the custom table.
    fn preset_definitions(&self) -> impl Iterator<Item = CustomInstrument> {
        self.presets().map(|instrument| {
            let (name, file) = instrument.nbs_definition().unwrap();
            CustomInstrument {
                name: name.into(),
                file: file.into(),
                pitch: 45,
                press_key: true,
            }
        })
    }

    /// The vanilla instrument matching a preset custom entry, if any.
    fn fold_instrument(&self, custom: &CustomInstrument) -> Option<Instrument> {
        self.presets().find(|instrument| {
            instrument.nbs_definition() == Some((custom.name.as_str(), custom.file.as_str()))
        })
    }

    /// Folds preset entries back into vanilla instruments, compressing the rest.
    fn fold_preset_instruments(
        &self,
        mut notes: Notes<Position, Note>,
        custom_instruments: Vec<CustomInstrument>,
    ) -> (Notes<Position, Note>, Vec<CustomInstrument>) {
        let fold_entry = |(slot, custom): (usize, &CustomInstrument)| {
            self.fold_instrument(custom)
                .map(|vanilla| (slot as u8, vanilla))
        };
        let folded: Vec<(u8, Instrument)> = custom_instruments
            .iter()
            .enumerate()
            .filter_map(fold_entry)
            .collect();
        if folded.is_empty() {
            return (notes, custom_instruments);
        }

        let keep_entry = |(slot, custom): (usize, CustomInstrument)| {
            let keep = folded
                .binary_search_by_key(&(slot as u8), |&(s, _)| s)
                .is_err();
            keep.then_some(custom)
        };
        let kept: Vec<CustomInstrument> = custom_instruments
            .into_iter()
            .enumerate()
            .filter_map(keep_entry)
            .collect();

        for (_, note) in notes.iter_mut() {
            let Instrument::Custom(slot) = note.tone().instrument() else {
                continue;
            };
            let instrument = folded
                .binary_search_by_key(&slot, |&(s, _)| s)
                .map(|index| folded[index].1)
                .unwrap_or_else(|insert| Instrument::Custom(slot - insert as u8));
            note.set_instrument(instrument);
        }

        (notes, kept)
    }
}

impl Middleware for InstrumentTranslate {
    fn decode_header(&mut self, header: Header) -> Header {
        self.version = Some(header.version);
        header
    }
    fn encode_header<'a>(&mut self, header: CowHeader<'a>) -> CowHeader<'a> {
        self.version = Some(header.version);
        header
    }

    /// Folds preset instruments into the song.
    fn decode_song(&mut self, mut song: Song) -> Song {
        (song.notes, song.custom_instruments) =
            self.fold_preset_instruments(song.notes, song.custom_instruments);
        song
    }

    /// Prepends the preset definitions to the custom table.
    fn encode_custom_insts<'a>(&mut self, customs: CowCustomInsts<'a>) -> CowCustomInsts<'a> {
        let mut presets = self.preset_definitions().peekable();
        match presets.peek() {
            None => customs,
            Some(_) => Cow::Owned(presets.chain(customs.iter().cloned()).collect()),
        }
    }

    /// Maps vanilla indices above the FCI into compatible custom slots.
    fn encode_instrument(&mut self, instrument: Instrument) -> Instrument {
        let version = self.version.unwrap();
        let fci = version.vanilla_instruments();
        debug_assert!(fci <= Instrument::vanilla_count());
        let offset = || Instrument::vanilla_count() - fci;
        let remap = |inst: Instrument| match inst.vanilla_index() {
            Some(i) if i >= fci => Instrument::Custom(i - fci),
            _ => inst,
        };
        match instrument {
            Instrument::Custom(slot) => Instrument::Custom(slot.saturating_add(offset())),
            // Instrument::Imitate(_) => unimplemented!(),
            inst => remap(inst),
        }
    }
}
