//! NBS (Note Block Studio) file format library for Rust.

use mcdata::GenericBlockState;

pub use crate::error::*;
pub use crate::util::*;

mod codec;
mod error;
mod nbs_ext;
#[cfg(test)]
mod tests;
mod util;
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::collections::HashMap;

// TEST
/// Pre-defined patterns for note block arrangement.
pub const PATTERNS: &[&[Index]] = &[
    &[0, 64, 128, 192, 32, 96, 160, 224],
    &[0, 64, 128, 192],
    &[0, 128],
    &[0],
];

// song
//
//

/// Represents a complete NBS song with header, notes, layers, and instruments.
#[derive(Debug, Default, Clone, PartialEq, PartialOrd)]
pub struct Song {
    pub header: Header,
    pub notes: Notes,
    pub layers: Layers,
    pub instruments: CustomInstruments,
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
//

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

// notes
//
//

/// A collection of notes in a song, indexed by their position (tick, layer).
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Notes(BTreeMap<(Index, Index), Note>);

impl Notes {
    /// Creates a new empty Notes collection.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a collection of references to all notes in the song.
    ///
    /// The return type can be any type that implements `FromIterator<&'a Note>`,
    /// such as `Vec<&Note>`, `HashSet<&Note>`, etc.
    pub fn get<'a, T: FromIterator<&'a Note>>(&'a self) -> T {
        self.0.values().collect()
    }

    /// Insert or replace the `Note` that already exists at that position
    pub fn insert(&mut self, note: Note) {
        self.0.insert((note.tick, note.layer), note);
    }

    /// Returns an iterator over the notes in the collection.
    pub fn iter(&self) -> impl Iterator<Item = &Note> {
        self.0.values()
    }

    /// Returns a mutable iterator over the notes in the collection.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Note> {
        self.0.values_mut()
    }
}

impl IntoIterator for Notes {
    type Item = Note;
    type IntoIter = std::collections::btree_map::IntoValues<(Index, Index), Note>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_values()
    }
}

impl<'a> IntoIterator for &'a Notes {
    type Item = &'a Note;
    type IntoIter = std::collections::btree_map::Values<'a, (Index, Index), Note>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.values()
    }
}

impl<'a> IntoIterator for &'a mut Notes {
    type Item = &'a mut Note;
    type IntoIter = std::collections::btree_map::ValuesMut<'a, (Index, Index), Note>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.values_mut()
    }
}

impl FromIterator<Note> for Notes {
    fn from_iter<T: IntoIterator<Item = Note>>(iter: T) -> Self {
        let mut notes = Notes::new();
        notes.extend(iter);
        notes
    }
}

impl Extend<Note> for Notes {
    /// Extends the collection with the contents of an iterator.
    ///
    /// This method adds all notes from the iterator to the collection.
    /// If multiple notes have the same position (tick, layer), the last one
    /// from the iterator will overwrite any previous ones.
    fn extend<T: IntoIterator<Item = Note>>(&mut self, iter: T) {
        for note in iter {
            self.insert(note);
        }
    }
}

impl From<Vec<Note>> for Notes {
    fn from(vec: Vec<Note>) -> Self {
        vec.into_iter().collect()
    }
}

impl From<Notes> for Vec<Note> {
    fn from(notes: Notes) -> Self {
        notes.into_iter().collect()
    }
}

// impl<'a> Extend<&'a Note> for Notes {
//     /// Extends the collection with references to notes from an iterator.
//     ///
//     /// This method clones all notes from the iterator and adds them to the collection.
//     /// If multiple notes have the same position (tick, layer), the last one
//     /// from the iterator will overwrite any previous ones.
//     fn extend<T: IntoIterator<Item = &'a Note>>(&mut self, iter: T) {
//         for note in iter {
//             self.insert(note.clone());
//         }
//     }
// }

// note
//
//

/// Represents a single note in the song with timing, instrument, and modulation data.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Note {
    // Position data is stored redundantly due to incremental encoding
    // I don't know why the header's song_length is only u16 :(
    pub tick: Index,
    pub layer: Index,
    pub instrument: Instrument,
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
            instrument: Instrument::default(),
            key: 33, // G3 #0
            velocity: Volume::default(),
            panning: Panning::default(),
            pitch: 0,
        }
    }
}

impl Note {
    /// Creates a new note with the specified position and tone parameters.
    pub fn new(tick: Index, layer: Index, instrument: Instrument, key: u8) -> Self {
        let mut note = Self::default();
        note.tick = tick;
        note.layer = layer;
        note.instrument = instrument;
        note.key = key;
        note
    }

    /// Returns the position of the note as a tuple (tick, layer)
    pub fn position(&self) -> (Index, Index) {
        (self.tick, self.layer)
    }

    /// Returns the tone of the note as a tuple (instrument, key)
    pub fn tone(&self) -> (Instrument, u8) {
        (self.instrument, self.key)
    }

    /// Returns the modulation parameters as a tuple (velocity, panning, pitch)
    pub fn modulation(&self) -> (u8, i8, i16) {
        (self.velocity.get(), self.panning.get(), self.pitch)
    }

    /// Returns the Minecraft note block block state for this note.
    pub fn note_block_state(&self) -> Option<GenericBlockState> {
        // Minecraft note block note range: 0 (F#3) to 24 (F#5)
        // NBS default key 33 = G3 = Minecraft note 0
        let note = self.key.checked_sub(33).filter(|&n| n <= 24)?;
        let properties = HashMap::from([
            (Cow::Borrowed("note"), Cow::Owned(note.to_string())),
            (Cow::Borrowed("powered"), Cow::Borrowed("false")),
            (
                Cow::Borrowed("instrument"),
                Cow::Borrowed(self.instrument.instrument_property()),
            ),
        ]);
        Some(GenericBlockState {
            name: Cow::Borrowed("minecraft:note_block"),
            properties,
        })
    }

    // /// Returns the block under the note block that determines the instrument's sound.
    // /// Returns `None` for custom instruments or mob head instruments.
    // pub fn instrument_block(&self) -> Option<GenericBlockState> {
    //     self.instrument.instrument_block()
    // }

    // /// Returns the mob head block for imitate (mob head) instruments.
    // /// Returns `None` for non-imitate instruments.
    // pub fn head_block(&self) -> Option<GenericBlockState> {
    //     self.instrument.head_block()
    // }
}

impl<T1, T2, T3, T4> TryFrom<(T1, T2, T3, T4)> for Note
where
    T1: TryInto<Index>,
    T2: TryInto<Index>,
    T3: TryInto<u8>,
    T4: TryInto<u8>,
    Error: From<T1::Error> + From<T2::Error> + From<T3::Error> + From<T4::Error>,
{
    type Error = Error;

    fn try_from((tick, layer, instrument, key): (T1, T2, T3, T4)) -> Result<Self> {
        let mut note = Self::default();
        note.tick = tick.try_into()?;
        note.layer = layer.try_into()?;
        note.instrument = instrument.try_into()?.into();
        note.key = key.try_into()?;
        Ok(note)
    }
}

// instrument
//
//

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

    fn from_u8(value: u8) -> Self {
        Self::TABLE
            .get(value as usize)
            .map(|(inst, _, _)| *inst)
            .unwrap_or(Self::Other(value))
    }

    fn to_u8(self) -> u8 {
        Self::TABLE
            .iter()
            .position(|(inst, _, _)| *inst == self)
            .map(|i| i as u8)
            .unwrap_or_else(|| match self {
                Self::Other(v) => v,
                _ => unreachable!(),
            })
    }
}

impl From<u8> for Instrument {
    fn from(value: u8) -> Self {
        Instrument::from_u8(value)
    }
}

impl From<Instrument> for u8 {
    fn from(instrument: Instrument) -> Self {
        Instrument::to_u8(instrument)
    }
}

// layer
//
//

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
//

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

// Basic Types
//
//

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

// pub type Notes = BTreeSet<Note>;
pub type Layers = Vec<Layer>;
pub type CustomInstruments = Vec<CustomInstrument>;

// pub type NotesRef<'a> = Vec<&'a Note>;
// pub type LayersRef<'a> = Vec<&'a Layer>;
// pub type InstrumentsRef<'a> = Vec<&'a Instrument>;

pub type Index = u32;
