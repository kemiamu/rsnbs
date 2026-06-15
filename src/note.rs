use mcdata::GenericBlockState;
use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::{self, Display, Formatter};

use crate::{Panning, Volume};

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

impl From<Tone> for Note {
    fn from((instrument, key): Tone) -> Self {
        Self::new(instrument, key)
    }
}

impl From<&Tone> for Note {
    fn from(tone: &Tone) -> Self {
        tone.clone().into()
    }
}

/// A tone is a pair of an instrument and a key.
pub type Tone = (Instrument, Key);

// instrument
//
// ============================================================================

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
    const TABLE: &'static [(Instrument, &'static str, &'static str)] = &[
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

impl Display for Instrument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.instrument_property())
    }
}

// key
//
// ============================================================================

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
        let clicks = self
            .minecraft_note()
            .map(|k| format!("{k:02} clicks"))
            .unwrap_or("invalid".into());
        write!(f, "{note}{octave} ({clicks})")
    }
}
