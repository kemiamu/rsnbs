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

/// Per-level transform hooks; parse calls map_*, write calls unmap_*.
pub(super) trait Middleware {
    fn map_header(&mut self, header: Header) -> Header {
        header
    }
    fn unmap_header<'a>(&mut self, header: CowHeader<'a>) -> CowHeader<'a> {
        header
    }

    fn map_notes(&mut self, notes: Notes<Position, Note>) -> Notes<Position, Note> {
        notes
    }
    fn unmap_notes<'a>(&mut self, notes: CowNotes<'a>) -> CowNotes<'a> {
        notes
    }

    fn map_note(&mut self, note: Note) -> Note {
        note
    }
    fn unmap_note<'a>(&mut self, note: CowNote<'a>) -> CowNote<'a> {
        note
    }

    fn map_layer(&mut self, layer: Layer) -> Layer {
        layer
    }
    fn unmap_layer<'a>(&mut self, layer: CowLayer<'a>) -> CowLayer<'a> {
        layer
    }

    fn map_custom_inst(&mut self, instrument: CustomInstrument) -> CustomInstrument {
        instrument
    }
    fn unmap_custom_inst<'a>(&mut self, instrument: CowCustomInst<'a>) -> CowCustomInst<'a> {
        instrument
    }

    fn map_version(&mut self, version: Version) -> Version {
        version
    }
    fn unmap_version(&mut self, version: Version) -> Version {
        version
    }

    fn map_instrument(&mut self, instrument: Instrument) -> Instrument {
        instrument
    }
    fn unmap_instrument(&mut self, instrument: Instrument) -> Instrument {
        instrument
    }

    fn map_volume(&mut self, volume: Volume) -> Volume {
        volume
    }
    fn unmap_volume(&mut self, volume: Volume) -> Volume {
        volume
    }

    fn map_key(&mut self, key: Key) -> Key {
        key
    }
    fn unmap_key(&mut self, key: Key) -> Key {
        key
    }

    fn map_panning(&mut self, panning: Panning) -> Panning {
        panning
    }
    fn unmap_panning(&mut self, panning: Panning) -> Panning {
        panning
    }

    fn map_f32(&mut self, value: f32) -> f32 {
        value
    }
    fn unmap_f32(&mut self, value: f32) -> f32 {
        value
    }
}

/// Identity chain tail.
impl Middleware for () {}

/// (A, B) pass-through; value for by-value hooks, cow for borrowed ones.
macro_rules! middleware_chain {
    ($method:ident, $ty:ty, value) => {
        fn $method(&mut self, value: $ty) -> $ty {
            let value = self.0.$method(value);
            self.1.$method(value)
        }
    };
    ($method:ident, $ty:ty, cow) => {
        fn $method<'a>(&mut self, value: Cow<'a, $ty>) -> Cow<'a, $ty> {
            let value = self.0.$method(value);
            self.1.$method(value)
        }
    };
}

/// Tuple combinator: chains two middlewares, .0 runs first.
impl<A: Middleware, B: Middleware> Middleware for (A, B) {
    middleware_chain!(map_header, Header, value);
    middleware_chain!(unmap_header, Header, cow);
    middleware_chain!(map_notes, Notes<Position, Note>, value);
    middleware_chain!(unmap_notes, Notes<Position, Note>, cow);
    middleware_chain!(map_note, Note, value);
    middleware_chain!(unmap_note, Note, cow);
    middleware_chain!(map_layer, Layer, value);
    middleware_chain!(unmap_layer, Layer, cow);
    middleware_chain!(map_custom_inst, CustomInstrument, value);
    middleware_chain!(unmap_custom_inst, CustomInstrument, cow);
    middleware_chain!(map_version, Version, value);
    middleware_chain!(unmap_version, Version, value);
    middleware_chain!(map_instrument, Instrument, value);
    middleware_chain!(unmap_instrument, Instrument, value);
    middleware_chain!(map_volume, Volume, value);
    middleware_chain!(unmap_volume, Volume, value);
    middleware_chain!(map_key, Key, value);
    middleware_chain!(unmap_key, Key, value);
    middleware_chain!(map_panning, Panning, value);
    middleware_chain!(unmap_panning, Panning, value);
    middleware_chain!(map_f32, f32, value);
    middleware_chain!(unmap_f32, f32, value);
}

// Song
//
// ++++++++++++============++++++++++++============++++++++++++============

impl Song {
    /// parses a complete Song from a reader
    pub fn parse<R: io::Read>(reader: &mut R) -> Result<Self> {
        let mut middlewares = ();
        Self::parse_with(reader, &mut middlewares)
    }

    /// writes the song to a writer.
    pub fn write<W: io::Write>(&self, writer: &mut W) -> Result<()> {
        let mut middlewares = ();
        self.write_with(writer, &mut middlewares)
    }
}
