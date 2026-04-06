//! NBS (Note Block Studio) file format library for Rust.
mod codec;
mod error;
mod nbs_ext;
#[cfg(test)]
mod tests;
use crate::codec::{Parser, Writer};
use nbs_ext::SaturatingCast;

pub use crate::error::{Error, Result};

// song
//
//

/// Represents a complete NBS song with header, notes, layers, and instruments.
#[derive(Debug, Default, Clone, PartialEq, PartialOrd)]
pub struct Song {
    pub header: Header,
    pub notes: Vec<Note>,
    pub layers: Vec<Layer>,
    pub instruments: Vec<Instrument>,
}

impl Song {
    /// Creates a new empty Song with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens and parses an NBS file from a file path
    pub fn open_nbs<P: AsRef<std::path::Path>>(path: P) -> Result<Self> {
        let mut file = std::fs::File::open(path)?;
        Self::parse(&mut file)
    }

    /// Saves the song to an NBS file at the specified path
    pub fn save_nbs<P: AsRef<std::path::Path>>(&mut self, path: P) -> Result<()> {
        self.refresh();
        let mut file = std::fs::File::create(path)?;
        self.write(&mut file)
    }

    /// Refreshes and updates the song to ensure data consistency
    pub fn refresh(&mut self) {
        // 从音符计算歌曲长度
        if let Some(last_note) = self.notes.iter().max_by_key(|n| n.tick) {
            self.header.song_length = last_note.tick.saturating_into();
        }

        // 更新 layer 数量
        self.header.song_layers = self.layers.len() as u16;

        // 对音符进行排序，先按 tick，再按 layer
        self.notes.sort();
    }
}

// header
//
//

/// Contains metadata and song information from the NBS file header.
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct Header {
    pub version: Version,
    pub default_instruments: u8,
    pub song_length: u16,
    pub song_layers: u16,
    pub song_name: String,
    pub song_author: String,
    pub original_author: String,
    pub description: String,
    pub tempo: f32,
    pub auto_save: bool,
    pub auto_save_duration: u8,
    pub time_signature: u8,
    pub minutes_spent: u32,
    pub left_clicks: u32,
    pub right_clicks: u32,
    pub blocks_added: u32,
    pub blocks_removed: u32,
    pub song_origin: String,
    pub is_loop: bool,
    pub max_loop_count: u8,
    pub loop_start: u16,
}

impl Default for Header {
    fn default() -> Self {
        Self {
            version: Version::default(),
            default_instruments: 16,
            song_length: 0,
            song_layers: 0,
            song_name: String::new(),
            song_author: String::new(),
            original_author: String::new(),
            description: String::new(),
            tempo: 10.0,
            auto_save: false,
            auto_save_duration: 10,
            time_signature: 4,
            minutes_spent: 0,
            left_clicks: 0,
            right_clicks: 0,
            blocks_added: 0,
            blocks_removed: 0,
            song_origin: String::new(),
            is_loop: false,
            max_loop_count: 0,
            loop_start: 0,
        }
    }
}

// note
//
//

/// Represents a single note in the song with timing, instrument, and modulation data.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Note {
    // Position data is stored redundantly due to incremental encoding
    // I don't know why the header's song_length is only u16 :(
    pub tick: u32,
    pub layer: u32,
    pub instrument: u8,
    pub key: u8,
    pub velocity: Volume,
    pub panning: Panning,
    pub pitch: i16,
}

impl Default for Note {
    fn default() -> Self {
        Self {
            tick: 0,
            layer: 0,
            instrument: 0,
            key: 33, // G3 #0
            velocity: Volume::default(),
            panning: Panning::default(),
            pitch: 0,
        }
    }
}

impl Note {
    /// Creates a new note with the specified position and tone parameters.
    pub fn new(tick: u32, layer: u32, instrument: u8, key: u8) -> Self {
        let mut note = Self::default();
        note.tick = tick;
        note.layer = layer;
        note.instrument = instrument;
        note.key = key;
        note
    }

    /// Returns the position of the note as a tuple (tick, layer)
    pub fn position(&self) -> (u32, u32) {
        (self.tick, self.layer)
    }

    /// Returns the tone of the note as a tuple (instrument, key)
    pub fn tone(&self) -> (u8, u8) {
        (self.instrument, self.key)
    }

    /// Returns the modulation parameters as a tuple (velocity, panning, pitch)
    pub fn modulation(&self) -> (u8, i8, i16) {
        (self.velocity.get(), self.panning.get(), self.pitch)
    }
}

// layer
//
//

/// Represents a layer in the song with volume, panning, and lock settings.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Layer {
    pub id: u16,
    pub name: String,
    pub lock: bool,
    pub volume: Volume,
    pub panning: Panning,
}

impl Default for Layer {
    fn default() -> Self {
        Self {
            id: 0,
            name: String::new(),
            lock: false,
            volume: Volume::default(),
            panning: Panning::default(),
        }
    }
}

// instrument
//
//

/// Represents an instrument with sound file, pitch, and playback settings.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Instrument {
    pub id: u8,
    pub name: String,
    pub file: String,
    pub pitch: u8,
    pub press_key: bool,
}

impl Default for Instrument {
    fn default() -> Self {
        Self {
            id: 0,
            name: String::new(),
            file: String::new(),
            pitch: 45,
            press_key: true,
        }
    }
}

// Basic Types
//
//

// /// Represents a MIDI key value in range 0-127
// #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
// pub struct Key(u8);

// impl Key {
//     pub fn new(key: u8) -> Result<Self> {
//         match key {
//             0..=127 => Ok(Self(key)),
//             _ => Err(Error::InvalidKey(key.to_string())),
//         }
//     }

//     pub fn get(&self) -> u8 {
//         self.0
//     }
// }

// impl Default for Key {
//     fn default() -> Self {
//         Self(0)
//     }
// }

/// The current NBS (Note Block Studio) file format version.
const CURRENT_NBS_VERSION: u8 = 5;

/// Represents a valid NBS version format
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version(u8);

impl Version {
    pub fn new(version: u8) -> Result<Self> {
        match version {
            0..=CURRENT_NBS_VERSION => Ok(Self(version)),
            _ => Err(Error::InvalidVersion(version.to_string())),
        }
    }

    pub fn get(&self) -> u8 {
        self.0.clone()
    }
}

impl Default for Version {
    fn default() -> Self {
        Self(CURRENT_NBS_VERSION)
    }
}

/// Represents Volume value in range 0-100
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Volume(u8);

impl Volume {
    pub fn new(volume: u8) -> Result<Self> {
        match volume {
            0..=100 => Ok(Self(volume)),
            _ => Err(Error::InvalidVolume(volume.to_string())),
        }
    }

    pub fn get(&self) -> u8 {
        self.0
    }
}

impl Default for Volume {
    fn default() -> Self {
        Self(100)
    }
}

/// Represents panning value in range -100 to 100
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Panning(i8);

impl Panning {
    pub fn new(panning: i8) -> Result<Self> {
        match &panning {
            (-100..=100) => Ok(Self(panning)),
            _ => Err(Error::InvalidPanning(panning.to_string())),
        }
    }

    pub fn get(&self) -> i8 {
        self.0
    }
}

impl Default for Panning {
    fn default() -> Self {
        Self(0)
    }
}
