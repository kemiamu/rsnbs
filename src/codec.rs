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

/// Chains a hook across (A, B): .0 runs first.
macro_rules! chain {
    ($hook:ident, $ty:ty) => {
        fn $hook(&mut self, value: $ty) -> $ty {
            let value = self.0.$hook(value);
            self.1.$hook(value)
        }
    };
    ($hook:ident, cow $ty:ident) => {
        fn $hook<'a>(&mut self, value: $ty<'a>) -> $ty<'a> {
            let value = self.0.$hook(value);
            self.1.$hook(value)
        }
    };
}

/// Tuple combinator: chains two middlewares, .0 runs first.
impl<A: Middleware, B: Middleware> Middleware for (A, B) {
    chain!(decode_header, Header);
    chain!(encode_header, cow CowHeader);
    chain!(decode_song, Song);
    chain!(encode_song, cow CowSong);
    chain!(decode_custom_insts, Vec<CustomInstrument>);
    chain!(encode_custom_insts, cow CowCustomInsts);
    chain!(decode_notes, Notes<Position, Note>);
    chain!(encode_notes, cow CowNotes);
    chain!(decode_note, Note);
    chain!(encode_note, cow CowNote);
    chain!(decode_layer, Layer);
    chain!(encode_layer, cow CowLayer);
    chain!(decode_custom_inst, CustomInstrument);
    chain!(encode_custom_inst, cow CowCustomInst);
    chain!(decode_version, Version);
    chain!(encode_version, Version);
    chain!(decode_instrument, Instrument);
    chain!(encode_instrument, Instrument);
    chain!(decode_volume, Volume);
    chain!(encode_volume, Volume);
    chain!(decode_key, Key);
    chain!(encode_key, Key);
    chain!(decode_panning, Panning);
    chain!(encode_panning, Panning);
    chain!(decode_f32, f32);
    chain!(encode_f32, f32);
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
