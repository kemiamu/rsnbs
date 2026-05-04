//! NBS (Note Block Studio) file format library for Rust.

mod codec;
mod error;
mod nbs_ext;
#[cfg(test)]
mod tests;
mod util;
use mcdata::GenericBlockState;
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::fmt::{self, Display, Formatter};

pub use crate::error::*;
pub use crate::util::*;

// TEST: temporary test case
/// Pre-defined patterns for note block arrangement.
pub const PATTERNS: &[&[Index]] = &[
    // &[0, 64, 128, 192, 32, 96, 160, 224],
    // &[0, 64, 128, 192],
    // &[0, 128],
    &[0, 24, 48, 72],
    &[0, 48],
    &[0],
];

// song
//
// ============================================================================

/// Represents a complete NBS song with header, notes, layers, and instruments.
#[derive(Debug, Default, Clone, PartialEq, PartialOrd)]
pub struct Song {
    pub header: Header,
    // Position data is stored redundantly due to incremental encoding
    // I don't know why the header's song_length is only u16 :(
    pub notes: BTreeMap<Position, Note>,
    pub layers: Vec<Layer>,
    pub custom_instruments: Vec<CustomInstrument>,
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

    /// Parses an NBS file from standard input
    pub fn from_stdin() -> Result<Self> {
        let mut stdin = std::io::stdin();
        Self::parse(&mut stdin)
    }

    /// Saves the song to an NBS file at the specified path
    pub fn save_nbs<P: AsRef<std::path::Path>>(&mut self, path: P) -> Result<()> {
        let mut file = std::fs::File::create(path)?;
        self.write(&mut file)
    }

    /// Writes the song to standard output
    pub fn to_stdout(&mut self) -> Result<()> {
        let mut stdout = std::io::stdout();
        self.write(&mut stdout)
    }
}

// header
//
// ============================================================================

/// Contains metadata and song information from the NBS file header.
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct Header {
    pub version: Version,
    pub default_instruments: u8,
    pub song_length: Index,
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
    pub loop_start: Index,
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
// ============================================================================

/// Represents a single note in the song with timing, instrument, and modulation data.
#[derive(Debug, Default, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Note {
    pub instrument: Instrument,
    pub key: Key,
    pub velocity: Volume,
    pub panning: Panning,
    pub pitch: i16,
}

impl Note {
    /// Creates a new note with the specified position and tone parameters.
    pub fn new(instrument: Instrument, key: Key) -> Self {
        let mut note = Self::default();
        note.instrument = instrument;
        note.key = key;
        note
    }

    /// Returns the tone of the note as a tuple (instrument, key)
    pub fn tone(&self) -> (Instrument, Key) {
        (self.instrument, self.key)
    }

    /// Returns the modulation parameters as a tuple (velocity, panning, pitch)
    pub fn modulation(&self) -> (u8, i8, i16) {
        (self.velocity.get(), self.panning.get(), self.pitch)
    }

    /// Returns the Minecraft note block block state for this note.
    pub fn note_block_state(&self) -> Option<GenericBlockState> {
        let note = self.key.minecraft_note()?;
        let instr = self.instrument.instrument_property();
        let properties = HashMap::from([
            ("note".into(), note.to_string().into()),
            ("powered".into(), "false".into()),
            ("instrument".into(), instr.into()),
        ]);
        Some(GenericBlockState {
            name: "minecraft:note_block".into(),
            properties,
        })
    }
}

/// Built-in Minecraft note block instruments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Instrument {
    Harp,
    DoubleBass,
    BassDrum,
    SnareDrum,
    Click,
    Guitar,
    Flute,
    Bell,
    Chime,
    Xylophone,
    IronXylophone,
    CowBell,
    Didgeridoo,
    Bit,
    Banjo,
    Pling,
    Trumpet,
    TrumpetExposed,
    TrumpetWeathered,
    TrumpetOxidized,
    // Mob head instruments
    Imitate(ImitateInstrument),
    // Other custom instruments
    Other(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ImitateInstrument {
    Creeper,
    Skeleton,
    Dragon,
    WitherSkeleton,
    Piglin,
    Zombie,
    CustomHead,
}

impl Default for Instrument {
    fn default() -> Self {
        Self::Harp
    }
}

impl Instrument {
    pub const TABLE: &'static [(Instrument, &'static str, &'static str)] = &[
        (
            //Harp
            Instrument::Harp,
            "harp",
            "minecraft:dirt",
        ),
        (
            //Double Bass
            Instrument::DoubleBass,
            "bass",
            "minecraft:oak_planks",
        ),
        (
            //Bass Drum
            Instrument::BassDrum,
            "basedrum",
            "minecraft:stone",
        ),
        (
            //Snare Drum
            Instrument::SnareDrum,
            "snare",
            "minecraft:sand",
        ),
        (
            //Click
            Instrument::Click,
            "hat",
            "minecraft:glass",
        ),
        (
            //Guitar
            Instrument::Guitar,
            "guitar",
            "minecraft:white_wool",
        ),
        (
            //Flute
            Instrument::Flute,
            "flute",
            "minecraft:clay",
        ),
        (
            //Bell
            Instrument::Bell,
            "bell",
            "minecraft:gold_block",
        ),
        (
            //Chime
            Instrument::Chime,
            "chime",
            "minecraft:packed_ice",
        ),
        (
            //Xylophone
            Instrument::Xylophone,
            "xylophone",
            "minecraft:bone_block",
        ),
        (
            //Iron Xylophone
            Instrument::IronXylophone,
            "iron_xylophone",
            "minecraft:iron_block",
        ),
        (
            //Cow Bell
            Instrument::CowBell,
            "cow_bell",
            "minecraft:soul_sand",
        ),
        (
            //Didgeridoo
            Instrument::Didgeridoo,
            "didgeridoo",
            "minecraft:pumpkin",
        ),
        (
            //Bit
            Instrument::Bit,
            "bit",
            "minecraft:emerald_block",
        ),
        (
            //Banjo
            Instrument::Banjo,
            "banjo",
            "minecraft:hay_block",
        ),
        (
            //Pling
            Instrument::Pling,
            "pling",
            "minecraft:glowstone",
        ),
        (
            //Trumpet
            Instrument::Trumpet,
            "trumpet",
            "minecraft:waxed_copper_block",
        ),
        (
            //Trumpet Exposed
            Instrument::TrumpetExposed,
            "trumpet_exposed",
            "minecraft:waxed_exposed_copper",
        ),
        (
            //Trumpet Weathered
            Instrument::TrumpetWeathered,
            "trumpet_weathered",
            "minecraft:waxed_weathered_copper",
        ),
        (
            //Trumpet Oxidized
            Instrument::TrumpetOxidized,
            "trumpet_oxidized",
            "minecraft:waxed_oxidized_copper",
        ),
        (
            //Creeper
            Instrument::Imitate(ImitateInstrument::Creeper),
            "creeper",
            "minecraft:creeper_head",
        ),
        (
            //Skeleton
            Instrument::Imitate(ImitateInstrument::Skeleton),
            "skeleton",
            "minecraft:skeleton_skull",
        ),
        (
            //Dragon
            Instrument::Imitate(ImitateInstrument::Dragon),
            "ender_dragon",
            "minecraft:dragon_head",
        ),
        (
            //Wither Skeleton
            Instrument::Imitate(ImitateInstrument::WitherSkeleton),
            "wither_skeleton",
            "minecraft:wither_skeleton_skull",
        ),
        (
            //Piglin
            Instrument::Imitate(ImitateInstrument::Piglin),
            "piglin",
            "minecraft:piglin_head",
        ),
        (
            //Zombie
            Instrument::Imitate(ImitateInstrument::Zombie),
            "zombie",
            "minecraft:zombie_head",
        ),
        (
            //Custom Head
            Instrument::Imitate(ImitateInstrument::CustomHead),
            "custom_head",
            "minecraft:player_head",
        ),
    ];

    /// Returns the instrument property string used in Minecraft's note block block state.
    pub fn instrument_property(&self) -> &'static str {
        Self::TABLE
            .iter()
            .find(|(inst, _, _)| inst == self)
            .map(|(_, prop, _)| *prop)
            .unwrap_or("custom")
    }

    /// Returns the block under the note block for this instrument's sound.
    pub fn instrument_block(&self) -> Option<GenericBlockState> {
        if matches!(self, Self::Imitate(_)) {
            return None;
        }
        let block = Self::TABLE
            .iter()
            .find(|(inst, _, _)| inst == self)
            .map(|(_, _, block)| *block)?;
        Some(GenericBlockState {
            name: Cow::Borrowed(block),
            properties: HashMap::new(),
        })
    }

    /// Returns the mob head block for this instrument, if it is a mob head instrument.
    pub fn head_block(&self) -> Option<GenericBlockState> {
        if !matches!(self, Self::Imitate(_)) {
            return None;
        }
        let block = Self::TABLE
            .iter()
            .find(|(inst, _, _)| inst == self)
            .map(|(_, _, block)| *block)?;
        Some(GenericBlockState {
            name: Cow::Borrowed(block),
            properties: HashMap::new(),
        })
    }
}

impl From<u8> for Instrument {
    fn from(value: u8) -> Self {
        Self::TABLE
            .get(value as usize)
            .map(|(inst, _, _)| *inst)
            .unwrap_or(Self::Other(value))
    }
}

impl From<Instrument> for u8 {
    fn from(instrument: Instrument) -> Self {
        Instrument::TABLE
            .iter()
            .position(|(inst, _, _)| *inst == instrument)
            .map(|i| i as u8)
            .unwrap_or_else(|| match instrument {
                Instrument::Other(v) => v,
                _ => unreachable!(),
            })
    }
}

// layer
//
// ============================================================================

/// Represents a layer in the song with volume, panning, and lock settings.
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
// ============================================================================

/// Represents an instrument with sound file, pitch, and playback settings.
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

// basic types
//
// ============================================================================

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

/// Represents a position in the NBS file, with a tick and layer index.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Position {
    tick: Index,
    layer: Index,
}

impl Position {
    pub fn new(tick: Index, layer: Index) -> Self {
        Self { tick, layer }
    }

    pub fn tick(&self) -> Index {
        self.tick
    }

    pub fn layer(&self) -> Index {
        self.layer
    }
}

/// Represents Volume value in range 0-100
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

/// Represents panning value in range -100 to 100
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

/// Represents a musical key (F#3-F#5)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Key(u8);

impl Default for Key {
    fn default() -> Self {
        Self::FS3
    }
}

impl Key {
    // F#3 = 33 = note(0)
    pub const FS3: Key = Key(33);
    pub const G3: Key = Key(34);
    pub const GS3: Key = Key(35);
    pub const A3: Key = Key(36);
    pub const AS3: Key = Key(37);
    pub const B3: Key = Key(38);
    pub const C4: Key = Key(39);
    pub const CS4: Key = Key(40);
    pub const D4: Key = Key(41);
    pub const DS4: Key = Key(42);
    pub const E4: Key = Key(43);
    pub const F4: Key = Key(44);
    pub const FS4: Key = Key(45);
    pub const G4: Key = Key(46);
    pub const GS4: Key = Key(47);
    pub const A4: Key = Key(48);
    pub const AS4: Key = Key(49);
    pub const B4: Key = Key(50);
    pub const C5: Key = Key(51);
    pub const CS5: Key = Key(52);
    pub const D5: Key = Key(53);
    pub const DS5: Key = Key(54);
    pub const E5: Key = Key(55);
    pub const F5: Key = Key(56);
    pub const FS5: Key = Key(57);

    pub fn new(key: u8) -> Self {
        Self(key)
    }

    /// Converts a Minecraft note (0-24, F#3-F#5) to the corresponding NBS Key.
    pub fn from_minecraft_note<T: TryInto<u8>>(note: T) -> Option<Self> {
        let key = note.try_into().ok()?.checked_add(33)?;
        if key <= 57 { Some(Self(key)) } else { None }
    }

    /// Converts the NBS Key to the corresponding Minecraft note (0-24, F#3-F#5).
    pub fn minecraft_note(&self) -> Option<u8> {
        self.0.checked_sub(33).filter(|&n| n <= 24)
    }
}

impl From<u8> for Key {
    fn from(value: u8) -> Self {
        Key(value)
    }
}

impl From<Key> for u8 {
    fn from(value: Key) -> Self {
        value.0
    }
}

impl Display for Key {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        const NOTE_NAMES: &[&str] = &[
            "A", "A#", "B", "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#",
        ];
        let note = NOTE_NAMES[(self.0 % 12) as usize];
        let octave = self.0 / 12;
        write!(f, "{note}{octave}")
    }
}

pub type Index = u32;
