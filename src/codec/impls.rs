//! Concrete read/write logic: song skeleton, preset folding, per-type codecs.

use super::nbs_ext::{NbsReadExt, NbsWriteExt};
use super::{Codec, Middleware};
use crate::note::{Instrument, Key, Note, Notes, Tone};
use crate::song::{CustomInstrument, Header, Layer, Song};
use crate::types::{Index, LayerAnchor, Panning, Position, Result};
use crate::types::{Tick, TickAnchor, Version, Volume};
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::io;
use std::num::NonZeroU32;

// Song
//
// ++++++++++++============++++++++++++============++++++++++++============

impl Song {
    /// parses a complete Song with a middleware chain
    pub(super) fn parse_with<R: io::Read, M: Middleware + ?Sized>(
        reader: &mut R,
        middlewares: &mut M,
    ) -> Result<Self> {
        // 头部分
        let header = Header::parse(reader, (), middlewares)?;
        let context = (header.version, header.default_instruments);

        // 音符部分
        let notes = Notes::parse(reader, context, middlewares)?;

        // 层部分
        let mut layers = Vec::new();
        for _ in 0..header.song_layers {
            layers.push(Layer::parse(reader, header.version, middlewares)?);
        }

        // 自定义乐器部分
        let instr_count = reader.read_u8()?;
        let mut custom_instruments = Vec::new();
        for _ in 0..instr_count {
            custom_instruments.push(CustomInstrument::parse(reader, (), middlewares)?);
        }
        let custom_instruments = middlewares.decode_custom_insts(custom_instruments);

        Ok(middlewares.decode_song(Song {
            header,
            notes,
            layers,
            custom_instruments,
        }))
    }

    /// writes the song with a middleware chain.
    pub(super) fn write_with<W: io::Write, M: Middleware + ?Sized>(
        &self,
        writer: &mut W,
        middlewares: &mut M,
    ) -> Result<()> {
        let song = middlewares.encode_song(Cow::Borrowed(self));
        let version = song.header.version;
        let fci = version.vanilla_instruments();
        let context = (version, fci);

        // 头部分
        Header::write(&song.header, writer, (), middlewares)?;

        // 音符部分
        song.notes.write(writer, context, middlewares)?;

        // 层部分
        for layer in &song.layers {
            layer.write(writer, version, middlewares)?;
        }

        let table = middlewares.encode_custom_insts(Cow::Borrowed(&song.custom_instruments));
        let count = table.len().min(255) as u8;
        writer.write_u8(count)?;
        for instr in table.iter().take(count as usize) {
            instr.write(writer, (), middlewares)?;
        }

        Ok(())
    }
}

impl Codec for Header {
    type Context = ();
    type Target = Self;

    fn parse<R: io::Read, M: Middleware + ?Sized>(
        reader: &mut R,
        _: Self::Context,
        middlewares: &mut M,
    ) -> Result<Self> {
        let mut header = Self::default();

        // 版本
        let song_length = reader.read_u16()?;
        header.version = Version::new(match song_length == 0 {
            true => reader.read_u8()?,
            false => 0,
        })?;

        header.default_instruments = match header.version.get() {
            0 => 10,
            _ => reader.read_u8()?,
        };

        header.song_length = match header.version.get() >= 3 {
            true => reader.read_u16()? as _,
            false => song_length as _,
        };

        // 头部分
        header.song_layers = reader.read_u16()? as _;
        header.song_name = reader.read_string()?;
        header.song_author = reader.read_string()?;
        header.original_author = reader.read_string()?;
        header.description = reader.read_string()?;
        header.tempo = f32::parse(reader, (), middlewares)?;
        header.auto_save = reader.read_bool()?;
        header.auto_save_duration = reader.read_u8()? as _;
        header.time_signature = reader.read_u8()?;
        header.minutes_spent = reader.read_u32()?;
        header.left_clicks = reader.read_u32()?;
        header.right_clicks = reader.read_u32()?;
        header.blocks_added = reader.read_u32()?;
        header.blocks_removed = reader.read_u32()?;
        header.song_origin = reader.read_string()?;

        // 循环部分
        if header.version.get() >= 4 {
            header.is_loop = reader.read_bool()?;
            header.max_loop_count = reader.read_u8()? as _;
            header.loop_start = reader.read_u16()? as _;
        }

        Ok(middlewares.decode_header(header))
    }

    fn write<W: io::Write, M: Middleware + ?Sized>(
        &self,
        writer: &mut W,
        _: Self::Context,
        middlewares: &mut M,
    ) -> Result<()> {
        let header = middlewares.encode_header(Cow::Borrowed(self));

        // 版本
        if header.version.get() > 0 {
            writer.write_u16(0)?;
            header.version.write(writer, (), middlewares)?;
            writer.write_u8(header.default_instruments)?;
        } else {
            writer.write_u16(header.song_length.max(1).try_into().unwrap_or(u16::MAX))?;
        }

        if header.version.get() >= 3 {
            writer.write_u16(header.song_length.try_into().unwrap_or(u16::MAX))?;
        }

        // 头部分
        writer.write_u16(header.song_layers.try_into().unwrap_or(u16::MAX))?;
        writer.write_string(&header.song_name)?;
        writer.write_string(&header.song_author)?;
        writer.write_string(&header.original_author)?;
        writer.write_string(&header.description)?;
        header.tempo.write(writer, (), middlewares)?;
        writer.write_bool(header.auto_save)?;
        writer.write_u8(header.auto_save_duration.try_into().unwrap_or(u8::MAX))?;
        writer.write_u8(header.time_signature)?;
        writer.write_u32(header.minutes_spent)?;
        writer.write_u32(header.left_clicks)?;
        writer.write_u32(header.right_clicks)?;
        writer.write_u32(header.blocks_added)?;
        writer.write_u32(header.blocks_removed)?;
        writer.write_string(&header.song_origin)?;

        // 循环部分
        if header.version.get() >= 4 {
            writer.write_bool(header.is_loop)?;
            writer.write_u8(header.max_loop_count.try_into().unwrap_or(u8::MAX))?;
            writer.write_u16(header.loop_start.try_into().unwrap_or(u16::MAX))?;
        }

        Ok(())
    }
}

// Notes
//
// ++++++++++++============++++++++++++============++++++++++++============

impl Codec for Notes<Position, Note> {
    type Context = (Version, u8);
    type Target = Self;

    fn parse<R: io::Read, M: Middleware + ?Sized>(
        reader: &mut R,
        context: Self::Context,
        middlewares: &mut M,
    ) -> Result<Self> {
        let mut notes = BTreeMap::new();

        // tick
        let mut tick_cursor = Tick::MAX;
        while let Some(tick_jump) = reader.read_jump()? {
            tick_cursor = tick_cursor.wrapping_add(tick_jump.get());

            // layer
            let mut layer_cursor = Index::MAX;
            while let Some(layer_jump) = reader.read_jump()? {
                layer_cursor = layer_cursor.wrapping_add(layer_jump.get());

                let note = Note::parse(reader, context, middlewares)?;
                notes.insert(Position::new(tick_cursor, layer_cursor), note);
            }
        }

        let notes = middlewares.decode_notes(notes.into());
        Ok(notes)
    }

    fn write<W: io::Write, M: Middleware + ?Sized>(
        &self,
        writer: &mut W,
        context: Self::Context,
        middlewares: &mut M,
    ) -> Result<()> {
        let notes = middlewares.encode_notes(Cow::Borrowed(self));
        let mut iter = notes.iter().peekable();
        let mut prev_tick = Tick::MAX;
        let mut prev_layer = Index::MAX;

        while let Some((pos, note)) = iter.next() {
            // tick 上升沿
            if pos.into_tick() != prev_tick {
                let tick_jump = pos.into_tick().wrapping_sub(prev_tick);
                writer.write_jump(NonZeroU32::new(tick_jump))?;
            }
            // layer 上升沿
            let layer_jump = pos.into_layer().wrapping_sub(prev_layer);
            writer.write_jump(NonZeroU32::new(layer_jump))?;

            note.write(writer, context, middlewares)?;
            prev_tick = pos.into_tick();
            prev_layer = pos.into_layer();
            // layer 下降沿
            let next_tick = iter.peek().map(|(pos, _)| pos.into_tick());
            if next_tick.is_none() || next_tick.unwrap() != pos.into_tick() {
                writer.write_jump(None)?;
                prev_layer = Index::MAX;
            }
        }
        // tick 下降沿
        writer.write_jump(None)?;

        Ok(())
    }
}

// Note
//
// ++++++++++++============++++++++++++============++++++++++++============

impl Codec for Note {
    type Context = (Version, u8);
    type Target = Self;

    fn parse<R: io::Read, M: Middleware + ?Sized>(
        reader: &mut R,
        context: Self::Context,
        middlewares: &mut M,
    ) -> Result<Self> {
        let (version, first_custom_index) = context;
        let mut note = Self::default();
        let instrument = Instrument::parse(reader, first_custom_index, middlewares)?;
        let key = Key::parse(reader, (), middlewares)?;
        note.tone = Tone::new(instrument, key);

        if version.get() >= 4 {
            note.velocity = Volume::parse(reader, (), middlewares)?;
            note.panning = Panning::parse(reader, (), middlewares)?;
            note.pitch = reader.read_i16()?;
        }

        let note = middlewares.decode_note(note);
        Ok(note)
    }

    fn write<W: io::Write, M: Middleware + ?Sized>(
        &self,
        writer: &mut W,
        context: Self::Context,
        middlewares: &mut M,
    ) -> Result<()> {
        let (version, first_custom_index) = context;
        let note = middlewares.encode_note(Cow::Borrowed(self));
        note.tone
            .instrument()
            .write(writer, first_custom_index, middlewares)?;
        writer.write_u8(note.tone.key().into())?;

        if version.get() >= 4 {
            note.velocity.write(writer, (), middlewares)?;
            note.panning.write(writer, (), middlewares)?;
            writer.write_i16(note.pitch)?;
        }

        Ok(())
    }
}

// Layer
//
// ++++++++++++============++++++++++++============++++++++++++============

impl Codec for Layer {
    type Context = Version;
    type Target = Self;

    /// parses a Layer from a reader with version context
    fn parse<R: io::Read, M: Middleware + ?Sized>(
        reader: &mut R,
        version: Self::Context,
        middlewares: &mut M,
    ) -> Result<Self> {
        let mut layer = Self::default();
        layer.name = reader.read_string()?;

        if version.get() >= 4 {
            layer.lock = reader.read_bool()?;
        }

        layer.volume = Volume::parse(reader, (), middlewares)?;

        if version.get() >= 2 {
            layer.panning = Panning::parse(reader, (), middlewares)?;
        }

        let layer = middlewares.decode_layer(layer);
        Ok(layer)
    }

    /// writes a Layer to a writer with version context
    fn write<W: io::Write, M: Middleware + ?Sized>(
        &self,
        writer: &mut W,
        version: Self::Context,
        middlewares: &mut M,
    ) -> Result<()> {
        let layer = middlewares.encode_layer(Cow::Borrowed(self));

        writer.write_string(&layer.name)?;

        if version.get() >= 4 {
            writer.write_bool(layer.lock)?;
        }

        layer.volume.write(writer, (), middlewares)?;

        if version.get() >= 2 {
            layer.panning.write(writer, (), middlewares)?;
        }

        Ok(())
    }
}

// Custom Instrument
//
// ++++++++++++============++++++++++++============++++++++++++============

impl Codec for CustomInstrument {
    type Context = ();
    type Target = Self;

    /// parses an Instrument from a reader
    fn parse<R: io::Read, M: Middleware + ?Sized>(
        reader: &mut R,
        _: Self::Context,
        middlewares: &mut M,
    ) -> Result<Self> {
        let mut instrument = Self::default();
        instrument.name = reader.read_string()?;
        instrument.file = reader.read_string()?;
        instrument.pitch = reader.read_u8()?;
        instrument.press_key = reader.read_bool()?;
        let instrument = middlewares.decode_custom_inst(instrument);
        Ok(instrument)
    }

    /// writes an Instrument to a writer
    fn write<W: io::Write, M: Middleware + ?Sized>(
        &self,
        writer: &mut W,
        _: Self::Context,
        middlewares: &mut M,
    ) -> Result<()> {
        let instrument = middlewares.encode_custom_inst(Cow::Borrowed(self));
        writer.write_string(&instrument.name)?;
        writer.write_string(&instrument.file)?;
        writer.write_u8(instrument.pitch)?;
        writer.write_bool(instrument.press_key)?;
        Ok(())
    }
}

// Basic Types
//
// ++++++++++++============++++++++++++============++++++++++++============

impl Codec for Version {
    type Context = ();
    type Target = Self;

    fn parse<R: io::Read, M: Middleware + ?Sized>(
        reader: &mut R,
        _: Self::Context,
        middlewares: &mut M,
    ) -> Result<Self> {
        let version = Version::new(reader.read_u8()?)?;
        let version = middlewares.decode_version(version);
        Ok(version)
    }

    fn write<W: io::Write, M: Middleware + ?Sized>(
        &self,
        writer: &mut W,
        _: Self::Context,
        middlewares: &mut M,
    ) -> Result<()> {
        let version = middlewares.encode_version(*self);
        Ok(writer.write_u8(version.get())?)
    }
}

impl Codec for Instrument {
    /// Byte index where custom instruments start.
    type Context = u8;
    type Target = Self;

    fn parse<R: io::Read, M: Middleware + ?Sized>(
        reader: &mut R,
        first_custom_index: Self::Context,
        middlewares: &mut M,
    ) -> Result<Self> {
        debug_assert!(first_custom_index <= Instrument::vanilla_count());
        let byte = reader.read_u8()?;
        let instrument = match byte < first_custom_index {
            true => Instrument::NBS_INDEX[byte as usize],
            false => Instrument::Custom(byte - first_custom_index),
        };
        let instrument = middlewares.decode_instrument(instrument);
        Ok(instrument)
    }

    fn write<W: io::Write, M: Middleware + ?Sized>(
        &self,
        writer: &mut W,
        first_custom_index: Self::Context,
        middlewares: &mut M,
    ) -> Result<()> {
        let instrument = middlewares.encode_instrument(*self);
        let byte = match instrument {
            Instrument::Custom(slot) => first_custom_index.saturating_add(slot),
            // Instrument::Imitate(_) => unimplemented!(),
            inst => inst
                .vanilla_index()
                .filter(|index| *index < first_custom_index)
                .unwrap_or(0),
        };
        writer.write_u8(byte)?;
        Ok(())
    }
}

impl Codec for Volume {
    type Context = ();
    type Target = Self;

    fn parse<R: io::Read, M: Middleware + ?Sized>(
        reader: &mut R,
        _: Self::Context,
        middlewares: &mut M,
    ) -> Result<Self> {
        let volume = Volume::new(reader.read_u8()?)?;
        let volume = middlewares.decode_volume(volume);
        Ok(volume)
    }

    fn write<W: io::Write, M: Middleware + ?Sized>(
        &self,
        writer: &mut W,
        _: Self::Context,
        middlewares: &mut M,
    ) -> Result<()> {
        let volume = middlewares.encode_volume(*self);
        Ok(writer.write_u8(volume.get())?)
    }
}

impl Codec for Key {
    type Context = ();
    type Target = Self;

    fn parse<R: io::Read, M: Middleware + ?Sized>(
        reader: &mut R,
        _: Self::Context,
        middlewares: &mut M,
    ) -> Result<Self> {
        let key: Key = reader.read_u8()?.into();
        let key = middlewares.decode_key(key);
        Ok(key)
    }

    fn write<W: io::Write, M: Middleware + ?Sized>(
        &self,
        writer: &mut W,
        _: Self::Context,
        middlewares: &mut M,
    ) -> Result<()> {
        let key = middlewares.encode_key(*self);
        Ok(writer.write_u8(key.into())?)
    }
}

impl Codec for Panning {
    type Context = ();
    type Target = Self;

    fn parse<R: io::Read, M: Middleware + ?Sized>(
        reader: &mut R,
        _: Self::Context,
        middlewares: &mut M,
    ) -> Result<Self> {
        let raw = reader.read_u8()?;
        // Convert from file representation (0-200) to internal (-100..100)
        let panning = Panning::new(raw.wrapping_sub(100) as i8)?;
        let panning = middlewares.decode_panning(panning);
        Ok(panning)
    }

    fn write<W: io::Write, M: Middleware + ?Sized>(
        &self,
        writer: &mut W,
        _: Self::Context,
        middlewares: &mut M,
    ) -> Result<()> {
        // Convert from internal (-100..100) to file representation (0-200)
        let panning = middlewares.encode_panning(*self);
        Ok(writer.write_u8((panning.get() as u8).wrapping_add(100))?)
    }
}

impl Codec for f32 {
    type Context = ();
    type Target = Self;

    fn parse<R: io::Read, M: Middleware + ?Sized>(
        reader: &mut R,
        _: Self::Context,
        middlewares: &mut M,
    ) -> Result<Self> {
        // Convert from u16 to f32 and divide by 100.0
        let value = reader.read_u16()? as f32 / 100.0;
        let value = middlewares.decode_f32(value);
        Ok(value)
    }

    fn write<W: io::Write, M: Middleware + ?Sized>(
        &self,
        writer: &mut W,
        _: Self::Context,
        middlewares: &mut M,
    ) -> Result<()> {
        // Convert f32 to u16 by multiplying by 100.0
        let value = middlewares.encode_f32(*self);
        Ok(writer.write_u16((value * 100.0) as u16)?)
    }
}
