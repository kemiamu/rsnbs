//! NBS (Note Block Studio) file format library for Rust.

mod codec;
mod nbs_ext;
#[cfg(test)]
mod tests;

pub mod note;
pub mod reuse;
pub mod schematic;
pub mod util;

// song
//
// ++++++++++++============++++++++++++============++++++++++++============

pub mod song {
    use crate::note::{Instrument, Note, Notes};
    use crate::types::{Index, IntoTick, Panning, Position, Result, Tick, Version, Volume};
    use std::collections::HashMap;

    /// represents a complete nbs song with header, notes, layers, and instruments.
    #[derive(Debug, Default, Clone, PartialEq, PartialOrd)]
    pub struct Song {
        pub header: Header,
        // Position data is stored redundantly due to incremental encoding
        // I don't know why the header's song_length is only u16 :(
        pub notes: Notes<Position, Note>,
        pub layers: Vec<Layer>,
        pub custom_instruments: Vec<CustomInstrument>,
    }

    impl Song {
        /// creates a new empty song with default values.
        pub fn new() -> Self {
            Self::default()
        }

        /// opens and parses an nbs file from a file path
        pub fn open_nbs<P: AsRef<std::path::Path>>(path: P) -> Result<Self> {
            let mut file = std::fs::File::open(path)?;
            Self::parse(&mut file)
        }

        /// parses an nbs file from standard input
        pub fn from_stdin() -> Result<Self> {
            let mut stdin = std::io::stdin();
            Self::parse(&mut stdin)
        }

        /// saves the song to an nbs file at the specified path
        pub fn save_nbs<P: AsRef<std::path::Path>>(&mut self, path: P) -> Result<()> {
            let mut file = std::fs::File::create(path)?;
            self.write(&mut file)
        }

        /// writes the song to standard output
        pub fn to_stdout(&mut self) -> Result<()> {
            let mut stdout = std::io::stdout();
            self.write(&mut stdout)
        }

        /// returns the tick count (max tick + 1) of the song.
        pub fn len(&self) -> Tick {
            self.notes
                .last_key_value()
                .map(|(p, _)| p.into_tick() + 1)
                .unwrap_or(0)
        }

        /// refreshes song data for consistency.
        pub fn refresh(&mut self) {
            // 更新歌曲长度
            self.header.song_length = self
                .notes
                .last_key_value()
                .map(|(p, _)| p.into_tick())
                .unwrap_or(1);
            // 更新 layer 数量
            self.header.song_layers = self.layers.len() as _;
            // 更新默认乐器数量
            self.header.default_instruments = self.header.version.vanilla_instruments();
        }

        /// converts vanilla instruments that do not exist in the given NBS
        /// version to custom instruments, so their sound survives saving the
        /// song in an older version.
        pub(crate) fn adapt_instruments_to_version(&mut self, version: Version) {
            let fci = version.vanilla_instruments();
            let unsupported = |instrument: Instrument| {
                instrument.vanilla_index().is_some_and(|index| index >= fci)
            };

            // 按出现顺序收集所有目标版本无法表示的原生乐器
            let mut needed: Vec<(&'static str, &'static str)> = Vec::new();
            for instrument in self.notes.values().map(|note| note.tone().instrument()) {
                if !unsupported(instrument) {
                    continue;
                }
                let Some(definition) = instrument.nbs_definition() else {
                    continue;
                };
                if !needed.contains(&definition) {
                    needed.push(definition);
                }
            }

            // 确保对应的自定义乐器存在,并记录槽位
            let mut slots: HashMap<(&'static str, &'static str), u8> = HashMap::new();
            for &(name, file) in &needed {
                let slot = match self
                    .custom_instruments
                    .iter()
                    .position(|ci| ci.name == name && ci.file == file)
                {
                    Some(existing) => existing as u8,
                    None => {
                        let slot = self.custom_instruments.len().min(u8::MAX as usize) as u8;
                        self.custom_instruments.push(CustomInstrument {
                            name: name.into(),
                            file: file.into(),
                            pitch: 45,
                            press_key: true,
                        });
                        slot
                    }
                };
                slots.insert((name, file), slot);
            }

            // 把受影响的音符改指自定义乐器槽位
            for note in self.notes.values_mut() {
                let instrument = note.tone().instrument();
                if !unsupported(instrument) {
                    continue;
                }
                let Some((name, file)) = instrument.nbs_definition() else {
                    continue;
                };
                let Some(&slot) = slots.get(&(name, file)) else {
                    continue;
                };
                note.set_instrument(Instrument::Custom(slot));
            }
        }
    }

    // header
    //
    // ++++++++++++============++++++++++++============++++++++++++============

    /// contains metadata and song information from the nbs file header.
    #[derive(Debug, Clone, PartialEq, PartialOrd)]
    pub struct Header {
        pub version: Version,
        pub default_instruments: u8,
        pub song_length: Tick,
        pub song_layers: Index,
        pub song_name: String,
        pub song_author: String,
        pub original_author: String,
        pub description: String,
        pub tempo: f32,
        pub auto_save: bool,
        pub auto_save_duration: u32,
        pub time_signature: u8,
        pub minutes_spent: u32,
        pub left_clicks: u32,
        pub right_clicks: u32,
        pub blocks_added: u32,
        pub blocks_removed: u32,
        pub song_origin: String,
        pub is_loop: bool,
        pub max_loop_count: u32,
        pub loop_start: Tick,
    }

    impl Default for Header {
        fn default() -> Self {
            Self {
                version: Version::default(),
                default_instruments: Version::default().vanilla_instruments(),
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

    // layer
    //
    // ++++++++++++============++++++++++++============++++++++++++============

    /// represents a layer with volume, panning, and lock settings.
    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Layer {
        pub name: String,
        pub lock: bool,
        pub volume: Volume,
        pub panning: Panning,
    }

    impl Default for Layer {
        fn default() -> Self {
            Self {
                name: String::new(),
                lock: false,
                volume: Volume::default(),
                panning: Panning::default(),
            }
        }
    }

    // custom instrument
    //
    // ++++++++++++============++++++++++++============++++++++++++============

    /// represents an instrument with sound file, pitch, and playback settings.
    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct CustomInstrument {
        pub name: String,
        pub file: String,
        pub pitch: u8,
        pub press_key: bool,
    }

    impl Default for CustomInstrument {
        fn default() -> Self {
            Self {
                name: String::new(),
                file: String::new(),
                pitch: 45,
                press_key: true,
            }
        }
    }
}

// types
//
// ++++++++++++============++++++++++++============++++++++++++============

pub mod types {
    /// the current nbs (note block studio) file format version.
    const CURRENT_NBS_VERSION: u8 = 6;

    /// represents a valid nbs version format
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
            self.0
        }

        /// the amount of vanilla instruments in this NBS version.
        /// version 6 added the four trumpet instruments.
        pub fn vanilla_instruments(self) -> u8 {
            match self.0 {
                0 => 10,
                1..=5 => 16,
                _ => 20,
            }
        }
    }

    impl Default for Version {
        fn default() -> Self {
            Self(CURRENT_NBS_VERSION)
        }
    }

    /// represents a position with tick and layer index.
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Position {
        tick: Tick,
        layer: Index,
    }

    impl Position {
        pub fn new(tick: Tick, layer: Index) -> Self {
            Self { tick, layer }
        }

        pub fn layer(self) -> Index {
            self.layer
        }
    }

    /// conversion into a tick value.
    pub trait IntoTick {
        fn into_tick(self) -> Tick;
    }

    impl<T: Into<Tick>> IntoTick for T {
        fn into_tick(self) -> Tick {
            self.into()
        }
    }

    impl IntoTick for Position {
        fn into_tick(self) -> Tick {
            self.tick
        }
    }

    impl IntoTick for &Position {
        fn into_tick(self) -> Tick {
            self.tick
        }
    }

    /// represents volume value in range 0-100
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Volume(u8);

    impl Default for Volume {
        fn default() -> Self {
            Self(100)
        }
    }

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

    /// represents panning value in range -100 to 100
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Panning(i8);

    impl Default for Panning {
        fn default() -> Self {
            Self(0)
        }
    }

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

    /// Represents the layer index
    pub type Index = u32;
    /// Represents the time step
    pub type Tick = u32;

    #[allow(dead_code)]
    pub(crate) type RedStoneTick = Tick;
    #[allow(dead_code)]
    pub(crate) type GameTick = Tick;

    // error
    //
    // ++++++++++++============++++++++++++============++++++++++++============

    /// custom error types
    #[derive(Debug, thiserror::Error)]
    pub enum Error {
        // #[error("Invalid key: {0}")]
        // InvalidKey(String),
        #[error("Invalid or unsupported version: {0}")]
        InvalidVersion(String),
        #[error("Invalid panning value: {0} (must be -100 to 100)")]
        InvalidPanning(String),
        #[error("Invalid velocity value: {0} (must be 0-100)")]
        InvalidVolume(String),
        #[error(transparent)]
        Io(#[from] std::io::Error),
    }

    pub type Result<T> = std::result::Result<T, Error>;
}
