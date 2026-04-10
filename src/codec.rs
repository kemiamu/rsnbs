//! NBS (Note Block Studio) file format parser and writer

use crate::nbs_ext::{NbsReadExt, NbsWriteExt};
use crate::util::Refreshable;
use crate::{Header, Instrument, Layer, Note, Notes, Song};
use crate::{Index, Panning, Result, Version, Volume};
use std::num::NonZeroU32;
use std::{io, u8, u16};

/// Data that can be parsed without state
pub(super) trait Parser {
    /// Parse data from a reader
    fn parse<R: io::Read>(reader: &mut R) -> Result<Self>
    where
        Self: Sized;
}

/// Data that can be written without state
pub(super) trait Writer {
    /// Write data to a writer
    fn write<W: io::Write>(&self, writer: &mut W) -> Result<()>;
}

/// Data that can be parsed with state
pub(super) trait StatefulParser<'a> {
    /// State type for parsing
    type ParseState;

    /// Parse data from a reader with state
    fn parse<R: io::Read>(reader: &mut R, state: Self::ParseState) -> Result<Self>
    where
        Self: Sized;
}

/// Data that can be written with state
pub(super) trait StatefulWriter<'a> {
    /// State type for writing
    type WriteState;

    /// Write data to a writer with state
    fn write<W: io::Write>(&self, writer: &mut W, state: Self::WriteState) -> Result<()>;
}

// Song
//
//

impl Song {
    /// Parses a complete Song from a reader
    pub fn parse<R: io::Read>(reader: &mut R) -> Result<Self> {
        let mut song = Self::default();

        // 头部分
        song.header = Header::parse(reader)?;

        // 音符部分
        song.notes = Notes::parse(reader, &song.header.version)?;

        // 层部分
        for _ in 0..song.header.song_layers {
            let layer = Layer::parse(reader, &song.header.version)?;
            song.layers.push(layer);
        }

        // 自定义乐器部分
        let instrument_count = reader.read_u8()?;
        for _ in 0..instrument_count {
            let instrument = Instrument::parse(reader)?;
            song.instruments.push(instrument);
        }

        Ok(song)
    }

    /// Writes the song to a writer after refreshing song data for consistency.
    pub fn write<W: io::Write>(&mut self, writer: &mut W) -> Result<()> {
        self.refresh();

        // 头部分
        self.header.write(writer)?;

        // 音符部分
        self.notes.write(writer, &self.header.version)?;

        // 层部分
        for layer in &self.layers {
            layer.write(writer, &self.header.version)?;
        }

        // 自定义乐器部分
        writer.write_u8(self.instruments.len().try_into().unwrap_or(u8::MAX))?;
        for instrument in self.instruments.iter().take(u8::MAX.into()) {
            instrument.write(writer)?;
        }

        Ok(())
    }
}

// Header
//
//

impl Parser for Header {
    /// Parses a Header from a reader
    fn parse<R: io::Read>(reader: &mut R) -> Result<Self> {
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
        header.tempo = f32::parse(reader)?;
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
}

impl Writer for Header {
    /// Writes a Header to a writer
    fn write<W: io::Write>(&self, writer: &mut W) -> Result<()> {
        // 版本
        if self.version.get() > 0 {
            writer.write_u16(0)?;
            self.version.write(writer)?;
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
        self.tempo.write(writer)?;
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
//

impl<'a, T: Default + Extend<Note>> StatefulParser<'a> for T {
    type ParseState = &'a Version;

    /// Parses notes from a reader with version state
    fn parse<R: io::Read>(reader: &mut R, version: Self::ParseState) -> Result<Self> {
        let mut notes = T::default();

        // tick
        let mut tick_cursor = Index::MAX;
        while let Some(tick_jump) = reader.read_jump()? {
            tick_cursor = tick_cursor.wrapping_add(tick_jump.get());

            // layer
            let mut layer_cursor = Index::MAX;
            while let Some(layer_jump) = reader.read_jump()? {
                layer_cursor = layer_cursor.wrapping_add(layer_jump.get());

                let note = Note::parse(reader, (version, tick_cursor, layer_cursor))?;
                notes.extend(std::iter::once(note));
            }
        }

        Ok(notes)
    }
}

impl<'a, T> StatefulWriter<'a> for T
where
    for<'b> &'b T: IntoIterator<Item = &'b Note>,
{
    type WriteState = &'a Version;

    /// Writes notes to a writer with version state
    ///
    /// # Assumptions
    /// - The notes are ordered by `(tick, layer)` (the natural order of `Note`).
    /// - No two notes share the same `(tick, layer)` pair.
    fn write<W: io::Write>(&self, writer: &mut W, version: Self::WriteState) -> Result<()> {
        let mut iter = self.into_iter().peekable();
        let mut prev_tick = Index::MAX;
        let mut prev_layer = Index::MAX;

        while let Some(note) = iter.next() {
            // tick 上升沿
            if note.tick != prev_tick {
                let tick_jump = note.tick.wrapping_sub(prev_tick);
                writer.write_jump(NonZeroU32::new(tick_jump))?;
            }
            // layer 上升沿
            let layer_jump = note.layer.wrapping_sub(prev_layer);
            writer.write_jump(NonZeroU32::new(layer_jump))?;

            note.write(writer, version)?;
            prev_tick = note.tick;
            prev_layer = note.layer;
            // layer 下降沿
            if iter.peek().map_or(true, |next| next.tick != note.tick) {
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
//

impl<'a> StatefulParser<'a> for Note {
    type ParseState = (&'a Version, Index, Index);

    /// Parses a Note from a reader with version, tick and layer state
    fn parse<R: io::Read>(reader: &mut R, state: Self::ParseState) -> Result<Self> {
        let (version, tick, layer) = state;
        let mut note = Self::default();

        note.instrument = reader.read_u8()?;
        note.key = reader.read_u8()?;
        note.tick = tick;
        note.layer = layer;

        if version.get() >= 4 {
            note.velocity = Volume::parse(reader)?;
            note.panning = Panning::parse(reader)?;
            note.pitch = reader.read_i16()?;
        }

        Ok(note)
    }
}

impl<'a> StatefulWriter<'a> for Note {
    type WriteState = &'a Version;

    /// Writes a Note to a writer with version state
    fn write<W: io::Write>(&self, writer: &mut W, version: Self::WriteState) -> Result<()> {
        writer.write_u8(self.instrument)?;
        writer.write_u8(self.key)?;

        if version.get() >= 4 {
            self.velocity.write(writer)?;
            self.panning.write(writer)?;
            writer.write_i16(self.pitch)?;
        }

        Ok(())
    }
}

// Layer
//
//

impl<'a> StatefulParser<'a> for Layer {
    type ParseState = &'a Version;

    /// Parses a Layer from a reader with version and id state
    fn parse<R: io::Read>(reader: &mut R, state: Self::ParseState) -> Result<Self> {
        let version = state;
        let mut layer = Self::default();
        layer.name = reader.read_string()?;

        if version.get() >= 4 {
            layer.lock = reader.read_bool()?;
        }

        layer.volume = Volume::parse(reader)?;

        if version.get() >= 2 {
            layer.panning = Panning::parse(reader)?;
        }

        Ok(layer)
    }
}

impl<'a> StatefulWriter<'a> for Layer {
    type WriteState = &'a Version;

    /// Writes a Layer to a writer with version state
    fn write<W: io::Write>(&self, writer: &mut W, version: Self::WriteState) -> Result<()> {
        writer.write_string(&self.name)?;

        if version.get() >= 4 {
            writer.write_bool(self.lock)?;
        }

        self.volume.write(writer)?;

        if version.get() >= 2 {
            self.panning.write(writer)?;
        }

        Ok(())
    }
}

// Instrument
//
//

impl Parser for Instrument {
    /// Parses an Instrument from a reader
    fn parse<R: io::Read>(reader: &mut R) -> Result<Self> {
        let mut instrument = Self::default();
        instrument.name = reader.read_string()?;
        instrument.file = reader.read_string()?;
        instrument.pitch = reader.read_u8()?;
        instrument.press_key = reader.read_bool()?;
        Ok(instrument)
    }
}

impl Writer for Instrument {
    /// Writes an Instrument to a writer
    fn write<W: io::Write>(&self, writer: &mut W) -> Result<()> {
        writer.write_string(&self.name)?;
        writer.write_string(&self.file)?;
        writer.write_u8(self.pitch)?;
        writer.write_bool(self.press_key)?;
        Ok(())
    }
}

// Basic Types
//
//

impl Parser for Version {
    fn parse<R: io::Read>(reader: &mut R) -> Result<Self> {
        Version::new(reader.read_u8()?)
    }
}

impl Writer for Version {
    fn write<W: io::Write>(&self, writer: &mut W) -> Result<()> {
        Ok(writer.write_u8(self.get())?)
    }
}

impl Parser for Volume {
    fn parse<R: io::Read>(reader: &mut R) -> Result<Self> {
        Volume::new(reader.read_u8()?)
    }
}

impl Writer for Volume {
    fn write<W: io::Write>(&self, writer: &mut W) -> Result<()> {
        Ok(writer.write_u8(self.get())?)
    }
}

impl Parser for Panning {
    fn parse<R: io::Read>(reader: &mut R) -> Result<Self> {
        let raw = reader.read_u8()?;
        // Convert from file representation (0-200) to internal (-100..100)
        Panning::new(raw.wrapping_sub(100) as i8)
    }
}

impl Writer for Panning {
    fn write<W: io::Write>(&self, writer: &mut W) -> Result<()> {
        // Convert from internal (-100..100) to file representation (0-200)
        Ok(writer.write_u8((self.get() as u8).wrapping_add(100))?)
    }
}

impl Parser for f32 {
    fn parse<R: io::Read>(reader: &mut R) -> Result<Self> {
        // Convert from u16 to f32 and divide by 100.0
        Ok(reader.read_u16()? as f32 / 100.0)
    }
}

impl Writer for f32 {
    fn write<W: io::Write>(&self, writer: &mut W) -> Result<()> {
        // Convert f32 to u16 by multiplying by 100.0
        Ok(writer.write_u16((self * 100.0) as u16)?)
    }
}
