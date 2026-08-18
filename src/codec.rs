//! NBS (Note Block Studio) file format parser and writer.

use crate::nbs_ext::{NbsReadExt, NbsWriteExt};
use crate::note::{Instrument, Key, Note, Notes, Tone};
use crate::song::{CustomInstrument, Header, Layer, Song};
use crate::types::{Index, IntoTick, Panning, Position, Result, Tick, Version, Volume};
use std::collections::BTreeMap;
use std::io;
use std::num::NonZeroU32;

/// unified trait for both parsing and writing data, optionally with context
pub(super) trait Codec {
    /// context type shared for both parsing and writing (use () when no context is needed)
    type Context: Copy;

    /// the type parse produces; usually Self, encoding wrappers override it with the wrapped type
    type Target;

    /// parse data from a reader with context
    fn parse<R: io::Read>(reader: &mut R, context: Self::Context) -> Result<Self::Target>;

    /// write data to a writer with context
    fn write<W: io::Write>(&self, writer: &mut W, context: Self::Context) -> Result<()>;
}

// Song
//
// ++++++++++++============++++++++++++============++++++++++++============

impl Song {
    /// parses a complete Song from a reader
    pub fn parse<R: io::Read>(reader: &mut R) -> Result<Self> {
        SongWriter::parse(reader, ())
    }

    /// writes the song to a writer.
    pub fn write<W: io::Write>(&self, writer: &mut W) -> Result<()> {
        SongWriter::wrap(self).write(writer, ())
    }
}

/// Write view borrowing a Song: derived header fields are computed while
/// writing, so encoding stays a zero-copy projection of the song state.
struct SongWriter<'a>(&'a Song);

impl SongWriter<'_> {
    /// Borrows the song without copying; the song itself is never modified.
    fn wrap(song: &Song) -> SongWriter<'_> {
        SongWriter(song)
    }
}

impl Codec for SongWriter<'_> {
    type Context = ();
    type Target = Song;

    fn parse<R: io::Read>(reader: &mut R, _: Self::Context) -> Result<Self::Target> {
        // 头部分
        let header = HeaderWriter::parse(reader, ())?;
        let context = (header.version, header.default_instruments);

        // 音符部分
        let notes = Notes::parse(reader, context)?;

        // 层部分
        let mut layers = Vec::new();
        for _ in 0..header.song_layers {
            layers.push(Layer::parse(reader, header.version)?);
        }

        // 自定义乐器部分
        let instr_count = reader.read_u8()?;
        let mut custom_instruments = Vec::new();
        for _ in 0..instr_count {
            custom_instruments.push(CustomInstrument::parse(reader, ())?);
        }

        // 折叠：高版本回退条目还原为原生乐器，其余自定义乐器压缩保序
        let (notes, custom_instruments) =
            fold_preset_instruments(header.version, notes, custom_instruments);

        Ok(Song {
            header,
            notes,
            layers,
            custom_instruments,
        })
    }

    fn write<W: io::Write>(&self, writer: &mut W, _: Self::Context) -> Result<()> {
        let Self(song) = self;
        let version = song.header.version;
        let fci = version.vanilla_instruments();
        let context = (version, fci);

        // 头部分
        HeaderWriter::wrap(song).write(writer, ())?;

        // 音符部分
        song.notes.write(writer, context)?;

        // 层部分
        for layer in &song.layers {
            layer.write(writer, version)?;
        }

        // 预设区在前（槽位 = 偏移，字节 = 原生索引），用户表在后（槽位 = 预设数 + 下标）
        let preset_count = Instrument::vanilla_count() - fci;
        let count = preset_count.saturating_add(song.custom_instruments.len() as u8);
        let custom_count = count.saturating_sub(preset_count);
        writer.write_u8(count)?;

        for instr in preset_definitions(version) {
            instr.write(writer, ())?;
        }
        for instr in song.custom_instruments.iter().take(custom_count as usize) {
            instr.write(writer, ())?;
        }

        Ok(())
    }
}

// Preset
//
// ++++++++++++============++++++++++++============++++++++++++============

/// All preset instruments the target version requires, written at the head
/// of the custom instrument table with fixed playback parameters.
fn preset_definitions(version: Version) -> impl Iterator<Item = CustomInstrument> {
    let fci = version.vanilla_instruments() as usize;
    let presets = Instrument::NBS_INDEX.into_iter().skip(fci);
    presets.map(|instrument| {
        let (name, file) = instrument.nbs_definition().unwrap();
        CustomInstrument {
            name: name.into(),
            file: file.into(),
            pitch: 45,
            press_key: true,
        }
    })
}

/// The vanilla instrument whose preset definition matches the custom entry,
/// if the target version actually presets it; such entries fold on read.
fn fold_instrument(version: Version, custom: &CustomInstrument) -> Option<Instrument> {
    let fci = version.vanilla_instruments() as usize;
    let mut presets = Instrument::NBS_INDEX.into_iter().skip(fci);
    presets.find(|instrument| {
        instrument.nbs_definition() == Some((custom.name.as_str(), custom.file.as_str()))
    })
}

/// Folds preset entries back into vanilla instruments, compressing the
/// remaining custom entries in order; returns unchanged when nothing matches.
fn fold_preset_instruments(
    version: Version,
    mut notes: Notes<Position, Note>,
    custom_instruments: Vec<CustomInstrument>,
) -> (Notes<Position, Note>, Vec<CustomInstrument>) {
    // 条目命中预设定义 → (槽位, 还原乐器)
    let fold_entry = |(slot, custom): (usize, &CustomInstrument)| {
        fold_instrument(version, custom).map(|vanilla| (slot as u8, vanilla))
    };
    let folded: Vec<(u8, Instrument)> = custom_instruments
        .iter()
        .enumerate()
        .filter_map(fold_entry)
        .collect();
    if folded.is_empty() {
        return (notes, custom_instruments);
    }

    // 槽位不在折叠表 → 保留条目
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

    // 重映射音符（就地修改，位置不变）
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

// Header
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Write view borrowing a Song's header: derived fields are computed from
/// the song while writing.
struct HeaderWriter<'a>(&'a Header, &'a Song);

impl HeaderWriter<'_> {
    /// Borrows the header; song length, layer count and default instrument
    /// count are derived from the song while writing.
    fn wrap(song: &Song) -> HeaderWriter<'_> {
        HeaderWriter(&song.header, song)
    }
}

impl Codec for HeaderWriter<'_> {
    type Context = ();
    type Target = Header;

    fn parse<R: io::Read>(reader: &mut R, _: Self::Context) -> Result<Self::Target> {
        let mut header = Header::default();

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
        header.tempo = f32::parse(reader, ())?;
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

    fn write<W: io::Write>(&self, writer: &mut W, _: Self::Context) -> Result<()> {
        let Self(header, song) = self;

        // 派生字段：歌曲长度、层数、默认乐器数
        let song_length = song
            .notes
            .last_key_value()
            .map(|(p, _)| p.into_tick())
            .unwrap_or(1);
        let song_layers = song.layers.len() as u32;
        let default_instruments = header.version.vanilla_instruments();

        // 版本
        if header.version.get() > 0 {
            writer.write_u16(0)?;
            header.version.write(writer, ())?;
            writer.write_u8(default_instruments)?;
        } else {
            writer.write_u16(song_length.max(1).try_into().unwrap_or(u16::MAX))?;
        }

        if header.version.get() >= 3 {
            writer.write_u16(song_length.try_into().unwrap_or(u16::MAX))?;
        }

        // 头部分
        writer.write_u16(song_layers.try_into().unwrap_or(u16::MAX))?;
        writer.write_string(&header.song_name)?;
        writer.write_string(&header.song_author)?;
        writer.write_string(&header.original_author)?;
        writer.write_string(&header.description)?;
        header.tempo.write(writer, ())?;
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

    fn parse<R: io::Read>(reader: &mut R, context: Self::Context) -> Result<Self> {
        let mut notes = BTreeMap::new();

        // tick
        let mut tick_cursor = Tick::MAX;
        while let Some(tick_jump) = reader.read_jump()? {
            tick_cursor = tick_cursor.wrapping_add(tick_jump.get());

            // layer
            let mut layer_cursor = Index::MAX;
            while let Some(layer_jump) = reader.read_jump()? {
                layer_cursor = layer_cursor.wrapping_add(layer_jump.get());

                let note = Note::parse(reader, context)?;
                notes.insert(Position::new(tick_cursor, layer_cursor), note);
            }
        }

        Ok(notes.into())
    }

    fn write<W: io::Write>(&self, writer: &mut W, context: Self::Context) -> Result<()> {
        let mut iter = self.iter().peekable();
        let mut prev_tick = Tick::MAX;
        let mut prev_layer = Index::MAX;

        while let Some((pos, note)) = iter.next() {
            // tick 上升沿
            if pos.into_tick() != prev_tick {
                let tick_jump = pos.into_tick().wrapping_sub(prev_tick);
                writer.write_jump(NonZeroU32::new(tick_jump))?;
            }
            // layer 上升沿
            let layer_jump = pos.layer().wrapping_sub(prev_layer);
            writer.write_jump(NonZeroU32::new(layer_jump))?;

            note.write(writer, context)?;
            prev_tick = pos.into_tick();
            prev_layer = pos.layer();
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

    fn parse<R: io::Read>(reader: &mut R, context: Self::Context) -> Result<Self> {
        let (version, first_custom_index) = context;
        let mut note = Self::default();
        let instrument = Instrument::parse(reader, first_custom_index)?;
        let key = Key::parse(reader, ())?;
        note.tone = Tone::new(instrument, key);

        if version.get() >= 4 {
            note.velocity = Volume::parse(reader, ())?;
            note.panning = Panning::parse(reader, ())?;
            note.pitch = reader.read_i16()?;
        }

        Ok(note)
    }

    fn write<W: io::Write>(&self, writer: &mut W, context: Self::Context) -> Result<()> {
        let (version, first_custom_index) = context;
        self.tone.instrument().write(writer, first_custom_index)?;
        writer.write_u8(self.tone.key().into())?;

        if version.get() >= 4 {
            self.velocity.write(writer, ())?;
            self.panning.write(writer, ())?;
            writer.write_i16(self.pitch)?;
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
    fn parse<R: io::Read>(reader: &mut R, version: Self::Context) -> Result<Self> {
        let mut layer = Self::default();
        layer.name = reader.read_string()?;

        if version.get() >= 4 {
            layer.lock = reader.read_bool()?;
        }

        layer.volume = Volume::parse(reader, ())?;

        if version.get() >= 2 {
            layer.panning = Panning::parse(reader, ())?;
        }

        Ok(layer)
    }

    /// writes a Layer to a writer with version context
    fn write<W: io::Write>(&self, writer: &mut W, version: Self::Context) -> Result<()> {
        writer.write_string(&self.name)?;

        if version.get() >= 4 {
            writer.write_bool(self.lock)?;
        }

        self.volume.write(writer, ())?;

        if version.get() >= 2 {
            self.panning.write(writer, ())?;
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
    fn parse<R: io::Read>(reader: &mut R, _: Self::Context) -> Result<Self> {
        let mut instrument = Self::default();
        instrument.name = reader.read_string()?;
        instrument.file = reader.read_string()?;
        instrument.pitch = reader.read_u8()?;
        instrument.press_key = reader.read_bool()?;
        Ok(instrument)
    }

    /// writes an Instrument to a writer
    fn write<W: io::Write>(&self, writer: &mut W, _: Self::Context) -> Result<()> {
        writer.write_string(&self.name)?;
        writer.write_string(&self.file)?;
        writer.write_u8(self.pitch)?;
        writer.write_bool(self.press_key)?;
        Ok(())
    }
}

// Basic Types
//
// ++++++++++++============++++++++++++============++++++++++++============

impl Codec for Version {
    type Context = ();
    type Target = Self;

    fn parse<R: io::Read>(reader: &mut R, _: Self::Context) -> Result<Self> {
        Version::new(reader.read_u8()?)
    }

    fn write<W: io::Write>(&self, writer: &mut W, _: Self::Context) -> Result<()> {
        Ok(writer.write_u8(self.get())?)
    }
}

impl Codec for Instrument {
    /// Byte index where custom instruments start.
    type Context = u8;
    type Target = Self;

    fn parse<R: io::Read>(reader: &mut R, first_custom_index: Self::Context) -> Result<Self> {
        debug_assert!(first_custom_index <= Instrument::vanilla_count());
        let byte = reader.read_u8()?;
        match byte < first_custom_index {
            true => Ok(Instrument::NBS_INDEX[byte as usize]),
            false => Ok(Instrument::Custom(byte.saturating_sub(first_custom_index))),
        }
    }

    fn write<W: io::Write>(&self, writer: &mut W, _: Self::Context) -> Result<()> {
        // 写端字节与版本无关（读端按 FCI 分界还原）：原生写索引，自定义写 20 + 槽位
        let byte = match *self {
            Instrument::Custom(slot) => Instrument::vanilla_count().saturating_add(slot),
            _ => self.vanilla_index().unwrap_or(0),
        };
        writer.write_u8(byte)?;
        Ok(())
    }
}

impl Codec for Volume {
    type Context = ();
    type Target = Self;

    fn parse<R: io::Read>(reader: &mut R, _: Self::Context) -> Result<Self> {
        Volume::new(reader.read_u8()?)
    }

    fn write<W: io::Write>(&self, writer: &mut W, _: Self::Context) -> Result<()> {
        Ok(writer.write_u8(self.get())?)
    }
}

impl Codec for Key {
    type Context = ();
    type Target = Self;

    fn parse<R: io::Read>(reader: &mut R, _: Self::Context) -> Result<Self> {
        Ok(reader.read_u8()?.into())
    }

    fn write<W: io::Write>(&self, writer: &mut W, _: Self::Context) -> Result<()> {
        Ok(writer.write_u8((*self).into())?)
    }
}

impl Codec for Panning {
    type Context = ();
    type Target = Self;

    fn parse<R: io::Read>(reader: &mut R, _: Self::Context) -> Result<Self> {
        let raw = reader.read_u8()?;
        // Convert from file representation (0-200) to internal (-100..100)
        Panning::new(raw.wrapping_sub(100) as i8)
    }

    fn write<W: io::Write>(&self, writer: &mut W, _: Self::Context) -> Result<()> {
        // Convert from internal (-100..100) to file representation (0-200)
        Ok(writer.write_u8((self.get() as u8).wrapping_add(100))?)
    }
}

impl Codec for f32 {
    type Context = ();
    type Target = Self;

    fn parse<R: io::Read>(reader: &mut R, _: Self::Context) -> Result<Self> {
        // Convert from u16 to f32 and divide by 100.0
        Ok(reader.read_u16()? as f32 / 100.0)
    }

    fn write<W: io::Write>(&self, writer: &mut W, _: Self::Context) -> Result<()> {
        // Convert f32 to u16 by multiplying by 100.0
        Ok(writer.write_u16((self * 100.0) as u16)?)
    }
}
