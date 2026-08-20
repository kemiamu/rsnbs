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
        let header = Self::parse_header(reader, middlewares)?;
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

    /// writes the song with a middleware chain.
    pub(super) fn write_with<W: io::Write, M: Middleware + ?Sized>(
        &self,
        writer: &mut W,
        middlewares: &mut M,
    ) -> Result<()> {
        let version = self.header.version;
        let fci = version.vanilla_instruments();
        let context = (version, fci);

        // 头部分
        self.write_header(writer, middlewares)?;

        // 音符部分
        self.notes.write(writer, context, middlewares)?;

        // 层部分
        for layer in &self.layers {
            layer.write(writer, version, middlewares)?;
        }

        // 预设区在前（槽位 = 偏移，字节 = 原生索引），用户表在后（槽位 = 预设数 + 下标）
        let preset_count = Instrument::vanilla_count() - fci;
        let count = preset_count.saturating_add(self.custom_instruments.len() as u8);
        let custom_count = count.saturating_sub(preset_count);
        writer.write_u8(count)?;

        for instr in preset_definitions(version) {
            instr.write(writer, (), middlewares)?;
        }
        for instr in self.custom_instruments.iter().take(custom_count as usize) {
            instr.write(writer, (), middlewares)?;
        }

        Ok(())
    }

    /// parses the header section from a reader.
    fn parse_header<R: io::Read, M: Middleware + ?Sized>(
        reader: &mut R,
        middlewares: &mut M,
    ) -> Result<Header> {
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

        let header = middlewares.map_header(header);
        Ok(header)
    }

    /// writes the header section with fields derived from the song state.
    fn write_header<W: io::Write, M: Middleware + ?Sized>(
        &self,
        writer: &mut W,
        middlewares: &mut M,
    ) -> Result<()> {
        let header = middlewares.unmap_header(Cow::Borrowed(&self.header));

        // 派生字段：歌曲长度、层数、默认乐器数
        let song_length = self
            .notes
            .last_key_value()
            .map(|(p, _)| p.into_tick())
            .unwrap_or(1);
        let song_layers = self.layers.len() as u32;
        let default_instruments = header.version.vanilla_instruments();

        // 版本
        if header.version.get() > 0 {
            writer.write_u16(0)?;
            header.version.write(writer, (), middlewares)?;
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

        let notes = middlewares.map_notes(notes.into());
        Ok(notes)
    }

    fn write<W: io::Write, M: Middleware + ?Sized>(
        &self,
        writer: &mut W,
        context: Self::Context,
        middlewares: &mut M,
    ) -> Result<()> {
        let notes = middlewares.unmap_notes(Cow::Borrowed(self));
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

        let note = middlewares.map_note(note);
        Ok(note)
    }

    fn write<W: io::Write, M: Middleware + ?Sized>(
        &self,
        writer: &mut W,
        context: Self::Context,
        middlewares: &mut M,
    ) -> Result<()> {
        let (version, first_custom_index) = context;
        let note = middlewares.unmap_note(Cow::Borrowed(self));
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

        let layer = middlewares.map_layer(layer);
        Ok(layer)
    }

    /// writes a Layer to a writer with version context
    fn write<W: io::Write, M: Middleware + ?Sized>(
        &self,
        writer: &mut W,
        version: Self::Context,
        middlewares: &mut M,
    ) -> Result<()> {
        let layer = middlewares.unmap_layer(Cow::Borrowed(self));

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
        let instrument = middlewares.map_custom_inst(instrument);
        Ok(instrument)
    }

    /// writes an Instrument to a writer
    fn write<W: io::Write, M: Middleware + ?Sized>(
        &self,
        writer: &mut W,
        _: Self::Context,
        middlewares: &mut M,
    ) -> Result<()> {
        let instrument = middlewares.unmap_custom_inst(Cow::Borrowed(self));
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
        let version = middlewares.map_version(version);
        Ok(version)
    }

    fn write<W: io::Write, M: Middleware + ?Sized>(
        &self,
        writer: &mut W,
        _: Self::Context,
        middlewares: &mut M,
    ) -> Result<()> {
        let version = middlewares.unmap_version(*self);
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
            false => Instrument::Custom(byte.saturating_sub(first_custom_index)),
        };
        let instrument = middlewares.map_instrument(instrument);
        Ok(instrument)
    }

    fn write<W: io::Write, M: Middleware + ?Sized>(
        &self,
        writer: &mut W,
        _: Self::Context,
        middlewares: &mut M,
    ) -> Result<()> {
        // 写端字节与版本无关（读端按 FCI 分界还原）：原生写索引，自定义写 20 + 槽位
        let instrument = middlewares.unmap_instrument(*self);
        let byte = match instrument {
            Instrument::Custom(slot) => Instrument::vanilla_count().saturating_add(slot),
            _ => instrument.vanilla_index().unwrap_or(0),
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
        let volume = middlewares.map_volume(volume);
        Ok(volume)
    }

    fn write<W: io::Write, M: Middleware + ?Sized>(
        &self,
        writer: &mut W,
        _: Self::Context,
        middlewares: &mut M,
    ) -> Result<()> {
        let volume = middlewares.unmap_volume(*self);
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
        let key = middlewares.map_key(key);
        Ok(key)
    }

    fn write<W: io::Write, M: Middleware + ?Sized>(
        &self,
        writer: &mut W,
        _: Self::Context,
        middlewares: &mut M,
    ) -> Result<()> {
        let key = middlewares.unmap_key(*self);
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
        let panning = middlewares.map_panning(panning);
        Ok(panning)
    }

    fn write<W: io::Write, M: Middleware + ?Sized>(
        &self,
        writer: &mut W,
        _: Self::Context,
        middlewares: &mut M,
    ) -> Result<()> {
        // Convert from internal (-100..100) to file representation (0-200)
        let panning = middlewares.unmap_panning(*self);
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
        let value = middlewares.map_f32(value);
        Ok(value)
    }

    fn write<W: io::Write, M: Middleware + ?Sized>(
        &self,
        writer: &mut W,
        _: Self::Context,
        middlewares: &mut M,
    ) -> Result<()> {
        // Convert f32 to u16 by multiplying by 100.0
        let value = middlewares.unmap_f32(*self);
        Ok(writer.write_u16((value * 100.0) as u16)?)
    }
}
