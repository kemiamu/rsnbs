//! NBS (Note Block Studio) file format parser and writer.

use crate::nbs_ext::{NbsReadExt, NbsWriteExt};
use crate::util::Refreshable;
use crate::{CustomInstrument, Header, Instrument, Key, Layer, Note, Position, Song};
use crate::{Index, Panning, Result, Version, Volume};
use std::collections::BTreeMap;
use std::io;
use std::num::NonZeroU32;

/// Unified trait for both parsing and writing data, optionally with context
pub(super) trait Codec {
    /// Context type shared for both parsing and writing (use () when no context is needed)
    type Context;

    /// Parse data from a reader with context
    fn parse<R: io::Read>(reader: &mut R, context: &Self::Context) -> Result<Self>
    where
        Self: Sized;

    /// Write data to a writer with context
    fn write<W: io::Write>(&self, writer: &mut W, context: &Self::Context) -> Result<()>;
}

// Song
//
// ============================================================================

impl Song {
    /// Parses a complete Song from a reader
    pub fn parse<R: io::Read>(reader: &mut R) -> Result<Self> {
        let mut song = Self::default();

        // 头部分
        song.header = Codec::parse(reader, &())?;

        // 音符部分
        song.notes = Codec::parse(reader, &song.header.version)?;

        // 层部分
        for _ in 0..song.header.song_layers {
            let layer = Codec::parse(reader, &song.header.version)?;
            song.layers.push(layer);
        }

        // 自定义乐器部分
        let instr_count = reader.read_u8()?;
        for _ in 0..instr_count {
            let instrument = Codec::parse(reader, &())?;
            song.custom_instruments.push(instrument);
        }

        Ok(song)
    }

    /// Writes the song to a writer after refreshing song data for consistency.
    pub fn write<W: io::Write>(&mut self, writer: &mut W) -> Result<()> {
        self.refresh();

        // 头部分
        self.header.write(writer, &())?;

        // 音符部分
        self.notes.write(writer, &self.header.version)?;

        // 层部分
        for layer in &self.layers {
            layer.write(writer, &self.header.version)?;
        }

        // 自定义乐器部分
        writer.write_u8(self.custom_instruments.len().try_into().unwrap_or(u8::MAX))?;
        for instr in self.custom_instruments.iter().take(u8::MAX.into()) {
            instr.write(writer, &())?;
        }

        Ok(())
    }
}

// Header
//
// ============================================================================

impl Codec for Header {
    type Context = ();

    fn parse<R: io::Read>(reader: &mut R, _: &Self::Context) -> Result<Self> {
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
        header.tempo = Codec::parse(reader, &())?;
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

        Ok(header)
    }

    fn write<W: io::Write>(&self, writer: &mut W, _: &Self::Context) -> Result<()> {
        // 版本
        if self.version.get() > 0 {
            writer.write_u16(0)?;
            self.version.write(writer, &())?;
            writer.write_u8(self.default_instruments)?;
        } else {
            writer.write_u16(self.song_length.try_into().unwrap_or(u16::MAX))?;
        }

        if self.version.get() >= 3 {
            writer.write_u16(self.song_length.try_into().unwrap_or(u16::MAX))?;
        }

        // 头部分
        writer.write_u16(self.song_layers.try_into().unwrap_or(u16::MAX))?;
        writer.write_string(&self.song_name)?;
        writer.write_string(&self.song_author)?;
        writer.write_string(&self.original_author)?;
        writer.write_string(&self.description)?;
        self.tempo.write(writer, &())?;
        writer.write_bool(self.auto_save)?;
        writer.write_u8(self.auto_save_duration.try_into().unwrap_or(u8::MAX))?;
        writer.write_u8(self.time_signature)?;
        writer.write_u32(self.minutes_spent)?;
        writer.write_u32(self.left_clicks)?;
        writer.write_u32(self.right_clicks)?;
        writer.write_u32(self.blocks_added)?;
        writer.write_u32(self.blocks_removed)?;
        writer.write_string(&self.song_origin)?;

        // 循环部分
        if self.version.get() >= 4 {
            writer.write_bool(self.is_loop)?;
            writer.write_u8(self.max_loop_count.try_into().unwrap_or(u8::MAX))?;
            writer.write_u16(self.loop_start.try_into().unwrap_or(u16::MAX))?;
        }

        Ok(())
    }
}

// Notes
//
// ============================================================================

impl Codec for BTreeMap<Position, Note> {
    type Context = Version;

    fn parse<R: io::Read>(reader: &mut R, version: &Self::Context) -> Result<Self> {
        let mut notes = BTreeMap::new();

        // tick
        let mut tick_cursor = Index::MAX;
        while let Some(tick_jump) = reader.read_jump()? {
            tick_cursor = tick_cursor.wrapping_add(tick_jump.get());

            // layer
            let mut layer_cursor = Index::MAX;
            while let Some(layer_jump) = reader.read_jump()? {
                layer_cursor = layer_cursor.wrapping_add(layer_jump.get());

                let note = Note::parse(reader, version)?;
                notes.insert(Position::new(tick_cursor, layer_cursor), note);
            }
        }

        Ok(notes)
    }

    fn write<W: io::Write>(&self, writer: &mut W, context: &Self::Context) -> Result<()> {
        let mut iter = self.iter().peekable();
        let mut prev_tick = Index::MAX;
        let mut prev_layer = Index::MAX;

        while let Some((pos, note)) = iter.next() {
            // tick 上升沿
            if pos.tick() != prev_tick {
                let tick_jump = pos.tick().wrapping_sub(prev_tick);
                writer.write_jump(NonZeroU32::new(tick_jump))?;
            }
            // layer 上升沿
            let layer_jump = pos.layer().wrapping_sub(prev_layer);
            writer.write_jump(NonZeroU32::new(layer_jump))?;

            note.write(writer, context)?;
            prev_tick = pos.tick();
            prev_layer = pos.layer();
            // layer 下降沿
            let next_tick = iter.peek().map(|(pos, _)| pos.tick());
            if next_tick.is_none() || next_tick.unwrap() != pos.tick() {
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
// ============================================================================

impl Codec for Note {
    type Context = Version;

    fn parse<R: io::Read>(reader: &mut R, version: &Self::Context) -> Result<Self> {
        let mut note = Self::default();
        note.instrument = Codec::parse(reader, &())?;
        note.key = Codec::parse(reader, &())?;

        if version.get() >= 4 {
            note.velocity = Codec::parse(reader, &())?;
            note.panning = Codec::parse(reader, &())?;
            note.pitch = reader.read_i16()?;
        }

        Ok(note)
    }

    fn write<W: io::Write>(&self, writer: &mut W, version: &Self::Context) -> Result<()> {
        self.instrument.write(writer, &())?;
        writer.write_u8(self.key.into())?;

        if version.get() >= 4 {
            self.velocity.write(writer, &())?;
            self.panning.write(writer, &())?;
            writer.write_i16(self.pitch)?;
        }

        Ok(())
    }
}

// Layer
//
// ============================================================================

impl Codec for Layer {
    type Context = Version;

    /// Parses a Layer from a reader with version context
    fn parse<R: io::Read>(reader: &mut R, version: &Self::Context) -> Result<Self> {
        let mut layer = Self::default();
        layer.name = reader.read_string()?;

        if version.get() >= 4 {
            layer.lock = reader.read_bool()?;
        }

        layer.volume = Codec::parse(reader, &())?;

        if version.get() >= 2 {
            layer.panning = Codec::parse(reader, &())?;
        }

        Ok(layer)
    }

    /// Writes a Layer to a writer with version context
    fn write<W: io::Write>(&self, writer: &mut W, version: &Self::Context) -> Result<()> {
        writer.write_string(&self.name)?;

        if version.get() >= 4 {
            writer.write_bool(self.lock)?;
        }

        self.volume.write(writer, &())?;

        if version.get() >= 2 {
            self.panning.write(writer, &())?;
        }

        Ok(())
    }
}

// Custom Instrument
//
// ============================================================================

impl Codec for CustomInstrument {
    type Context = ();

    /// Parses an Instrument from a reader
    fn parse<R: io::Read>(reader: &mut R, _: &Self::Context) -> Result<Self> {
        let mut instrument = Self::default();
        instrument.name = reader.read_string()?;
        instrument.file = reader.read_string()?;
        instrument.pitch = reader.read_u8()?;
        instrument.press_key = reader.read_bool()?;
        Ok(instrument)
    }

    /// Writes an Instrument to a writer
    fn write<W: io::Write>(&self, writer: &mut W, _: &Self::Context) -> Result<()> {
        writer.write_string(&self.name)?;
        writer.write_string(&self.file)?;
        writer.write_u8(self.pitch)?;
        writer.write_bool(self.press_key)?;
        Ok(())
    }
}

// Basic Types
//
// ============================================================================

impl Codec for Version {
    type Context = ();

    fn parse<R: io::Read>(reader: &mut R, _: &Self::Context) -> Result<Self> {
        Version::new(reader.read_u8()?)
    }

    fn write<W: io::Write>(&self, writer: &mut W, _: &Self::Context) -> Result<()> {
        Ok(writer.write_u8(self.get())?)
    }
}

impl Codec for Volume {
    type Context = ();

    fn parse<R: io::Read>(reader: &mut R, _: &Self::Context) -> Result<Self> {
        Volume::new(reader.read_u8()?)
    }

    fn write<W: io::Write>(&self, writer: &mut W, _: &Self::Context) -> Result<()> {
        Ok(writer.write_u8(self.get())?)
    }
}

impl Codec for Instrument {
    type Context = ();

    fn parse<R: io::Read>(reader: &mut R, _: &Self::Context) -> Result<Self> {
        Ok(reader.read_u8()?.into())
    }

    fn write<W: io::Write>(&self, writer: &mut W, _: &Self::Context) -> Result<()> {
        Ok(writer.write_u8((*self).into())?)
    }
}

impl Codec for Key {
    type Context = ();

    fn parse<R: io::Read>(reader: &mut R, _: &Self::Context) -> Result<Self> {
        Ok(reader.read_u8()?.into())
    }

    fn write<W: io::Write>(&self, writer: &mut W, _: &Self::Context) -> Result<()> {
        Ok(writer.write_u8((*self).into())?)
    }
}

impl Codec for Panning {
    type Context = ();

    fn parse<R: io::Read>(reader: &mut R, _: &Self::Context) -> Result<Self> {
        let raw = reader.read_u8()?;
        // Convert from file representation (0-200) to internal (-100..100)
        Panning::new(raw.wrapping_sub(100) as i8)
    }

    fn write<W: io::Write>(&self, writer: &mut W, _: &Self::Context) -> Result<()> {
        // Convert from internal (-100..100) to file representation (0-200)
        Ok(writer.write_u8((self.get() as u8).wrapping_add(100))?)
    }
}

impl Codec for f32 {
    type Context = ();

    fn parse<R: io::Read>(reader: &mut R, _: &Self::Context) -> Result<Self> {
        // Convert from u16 to f32 and divide by 100.0
        Ok(reader.read_u16()? as f32 / 100.0)
    }

    fn write<W: io::Write>(&self, writer: &mut W, _: &Self::Context) -> Result<()> {
        // Convert f32 to u16 by multiplying by 100.0
        Ok(writer.write_u16((self * 100.0) as u16)?)
    }
}
