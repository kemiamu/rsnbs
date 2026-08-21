//! NBS (Note Block Studio) file format parser and writer.

use crate::note::{Instrument, Note, Notes};
use crate::song::{CustomInstrument, Header, Layer, Song};
use crate::types::{Index, Position, Result, Tick, TickAnchor, Version};
use std::borrow::Cow;
use std::io;

mod impls;
mod nbs_ext;

type CowHeader<'a> = Cow<'a, Header>;
type CowNote<'a> = Cow<'a, Note>;
type CowLayer<'a> = Cow<'a, Layer>;
type CowCustomInsts<'a> = Cow<'a, [CustomInstrument]>;
type CowCustomInst<'a> = Cow<'a, CustomInstrument>;
type CowSong<'a> = Cow<'a, Song>;

// Parse/Write
//
// ++++++++++++============++++++++++++============++++++++++++============

/// unified trait for both parsing and writing data, optionally with context
pub(super) trait Codec: Clone {
    /// context type shared for both parsing and writing (use () when no context is needed)
    type Context: Copy;

    /// parse data from a reader with context
    fn parse<R: io::Read, M: Transformer + ?Sized>(
        reader: &mut R,
        context: Self::Context,
        hooks: &mut M,
    ) -> Result<Self>;

    /// write data to a writer with context
    fn write<W: io::Write, M: Transformer + ?Sized>(
        &self,
        writer: &mut W,
        context: Self::Context,
        hooks: &mut M,
    ) -> Result<()>;
}

// Transformer
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Per-level transform hooks; parse calls decode_*, write calls encode_*.
pub(super) trait Transformer {
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

    fn decode_custom_insts(&mut self, customs: Vec<CustomInstrument>) -> Vec<CustomInstrument> {
        customs
    }
    fn encode_custom_insts<'a>(&mut self, customs: CowCustomInsts<'a>) -> CowCustomInsts<'a> {
        customs
    }

    fn decode_custom_inst(&mut self, instrument: CustomInstrument) -> CustomInstrument {
        instrument
    }
    fn encode_custom_inst<'a>(&mut self, instrument: CowCustomInst<'a>) -> CowCustomInst<'a> {
        instrument
    }

    fn decode_instrument(&mut self, instrument: Instrument) -> Instrument {
        instrument
    }
    fn encode_instrument(&mut self, instrument: Instrument) -> Instrument {
        instrument
    }
}

/// Identity chain tail.
impl Transformer for () {}

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

/// Tuple combinator: chains two hooks, .0 runs first.
impl<A: Transformer, B: Transformer> Transformer for (A, B) {
    chain!(decode_header, Header);
    chain!(encode_header, cow CowHeader);
    chain!(decode_song, Song);
    chain!(encode_song, cow CowSong);
    chain!(decode_custom_insts, Vec<CustomInstrument>);
    chain!(encode_custom_insts, cow CowCustomInsts);
    chain!(decode_note, Note);
    chain!(encode_note, cow CowNote);
    chain!(decode_layer, Layer);
    chain!(encode_layer, cow CowLayer);
    chain!(decode_custom_inst, CustomInstrument);
    chain!(encode_custom_inst, cow CowCustomInst);
    chain!(decode_instrument, Instrument);
    chain!(encode_instrument, Instrument);
}

// Song
//
// ++++++++++++============++++++++++++============++++++++++++============

impl Song {
    /// parses a complete Song from a reader
    pub fn parse<R: io::Read>(reader: &mut R) -> Result<Self> {
        let mut hooks = (InstrumentTranslate::new(), HeaderStats::new());
        Codec::parse(reader, (), &mut hooks)
    }

    /// writes the song to a writer.
    pub fn write<W: io::Write>(&self, writer: &mut W) -> Result<()> {
        let mut hooks = (InstrumentTranslate::new(), HeaderStats::new());
        Codec::write(self, writer, (), &mut hooks)
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

impl Transformer for InstrumentTranslate {
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

// HeaderStats
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Derives song statistics into the header on write.
pub(super) struct HeaderStats {
    song_length: Option<Tick>,
    song_layers: Option<Index>,
}

impl HeaderStats {
    pub(super) fn new() -> Self {
        HeaderStats {
            song_length: None,
            song_layers: None,
        }
    }
}

impl Transformer for HeaderStats {
    fn encode_song<'a>(&mut self, song: CowSong<'a>) -> CowSong<'a> {
        let last = song.notes.last_key_value();
        self.song_length = Some(last.map(|(p, _)| p.into_tick()).unwrap_or(1));
        self.song_layers = Some(song.layers.len() as _);
        song
    }
    fn encode_header<'a>(&mut self, header: CowHeader<'a>) -> CowHeader<'a> {
        let mut header = header.into_owned();
        header.song_length = self.song_length.unwrap();
        header.song_layers = self.song_layers.unwrap();
        header.default_instruments = header.version.vanilla_instruments();
        Cow::Owned(header)
    }
}
