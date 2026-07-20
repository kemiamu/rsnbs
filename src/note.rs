use crate::types::{Index, Panning, Position, Tick, Volume};
use itertools::Itertools;
use std::collections::BTreeMap;
use std::fmt::{self, Display, Formatter};
use std::ops::{Deref, DerefMut};

// note
//
// ++++++++++++============++++++++++++============++++++++++++============

/// a single note with timing, instrument, and modulation data.
#[derive(Debug, Default, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Note {
    pub(super) instrument: Instrument,
    pub(super) key: Key,
    pub(super) velocity: Volume,
    pub(super) panning: Panning,
    pub(super) pitch: i16,
}

impl Note {
    /// creates a note from any value that can convert into one.
    pub fn new(value: impl Into<Self>) -> Self {
        value.into()
    }

    /// returns the tone as a pair of instrument and key.
    pub fn tone(&self) -> Tone {
        Tone::new(self.instrument, self.key)
    }

    /// returns the modulation parameters of the note.
    pub fn modulation(&self) -> Modulation {
        Modulation {
            velocity: self.velocity,
            panning: self.panning,
            pitch: self.pitch,
        }
    }
}

impl From<Tone> for Note {
    fn from(tone: Tone) -> Self {
        Self {
            instrument: tone.instrument,
            key: tone.key,
            ..Default::default()
        }
    }
}

impl From<&Tone> for Note {
    fn from(tone: &Tone) -> Self {
        tone.clone().into()
    }
}

/// a tone is a pair of an instrument and a key.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Tone {
    instrument: Instrument,
    key: Key,
}

impl Tone {
    pub fn new(instrument: Instrument, key: Key) -> Self {
        Self { instrument, key }
    }

    pub fn instrument(&self) -> Instrument {
        self.instrument
    }

    pub fn key(&self) -> Key {
        self.key
    }
}

/// velocity, panning, and pitch of a note.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Modulation {
    pub velocity: Volume,
    pub panning: Panning,
    pub pitch: i16,
}

// instrument
//
// ++++++++++++============++++++++++++============++++++++++++============

/// built-in minecraft note block instruments.
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
    /// Maps NBS file instrument index → (variant, Minecraft property, Minecraft block).
    /// The array position is the NBS serialization index.
    const NBS_INDEX: &'static [Instrument] = &[
        Instrument::Harp,
        Instrument::DoubleBass,
        Instrument::BassDrum,
        Instrument::SnareDrum,
        Instrument::Click,
        Instrument::Guitar,
        Instrument::Flute,
        Instrument::Bell,
        Instrument::Chime,
        Instrument::Xylophone,
        Instrument::IronXylophone,
        Instrument::CowBell,
        Instrument::Didgeridoo,
        Instrument::Bit,
        Instrument::Banjo,
        Instrument::Pling,
        Instrument::Trumpet,
        Instrument::TrumpetExposed,
        Instrument::TrumpetWeathered,
        Instrument::TrumpetOxidized,
        Instrument::Imitate(ImitateInstrument::Creeper),
        Instrument::Imitate(ImitateInstrument::Skeleton),
        Instrument::Imitate(ImitateInstrument::Dragon),
        Instrument::Imitate(ImitateInstrument::WitherSkeleton),
        Instrument::Imitate(ImitateInstrument::Piglin),
        Instrument::Imitate(ImitateInstrument::Zombie),
        Instrument::Imitate(ImitateInstrument::CustomHead),
    ];
}

impl From<u8> for Instrument {
    fn from(value: u8) -> Self {
        Self::NBS_INDEX
            .get(value as usize)
            .copied()
            .unwrap_or(Self::Other(value))
    }
}

impl From<Instrument> for u8 {
    fn from(instrument: Instrument) -> Self {
        let idx = Instrument::NBS_INDEX
            .iter()
            .position(|&inst| inst == instrument)
            .map(|i| i as u8);
        idx.unwrap_or_else(|| match instrument {
            Instrument::Other(v) => v,
            _ => unreachable!(),
        })
    }
}

impl Display for Instrument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

// key
//
// ++++++++++++============++++++++++++============++++++++++++============

/// a musical key (f#3-f#5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Key(u8);

impl Default for Key {
    fn default() -> Self {
        Self::FS3
    }
}

macro_rules! keys {
    ($($name:ident = $value:expr),* $(,)?) => {
        $( pub const $name: Key = Key($value); )*
    };
}

impl Key {
    // F#3 = 33 = note(0)
    keys! {
        FS3 = 33, G3 = 34, GS3 = 35, A3 = 36, AS3 = 37, B3 = 38,
        C4 = 39, CS4 = 40, D4 = 41, DS4 = 42, E4 = 43, F4 = 44, FS4 = 45,
        G4 = 46, GS4 = 47, A4 = 48, AS4 = 49, B4 = 50,
        C5 = 51, CS5 = 52, D5 = 53, DS5 = 54, E5 = 55, F5 = 56, FS5 = 57,
    }

    pub fn new(key: u8) -> Self {
        Self(key)
    }

    /// converts a minecraft note (0-24, f#3-f#5) to the corresponding nbs key.
    pub fn from_minecraft_note<T: TryInto<u8>>(note: T) -> Option<Self> {
        let key = note.try_into().ok()?.checked_add(33)?;
        if key <= 57 { Some(Self(key)) } else { None }
    }

    /// converts the nbs key to the corresponding minecraft note (0-24, f#3-f#5).
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

// notes collection
//
// ++++++++++++============++++++++++++============++++++++++++============

/// ordered note set, guarantees position order for nbs serialization.
#[derive(Debug, Default, Clone, PartialEq, PartialOrd)]
pub struct Notes(BTreeMap<Position, Note>);

impl Deref for Notes {
    type Target = BTreeMap<Position, Note>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Notes {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl IntoIterator for Notes {
    type Item = (Position, Note);
    type IntoIter = std::collections::btree_map::IntoIter<Position, Note>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a Notes {
    type Item = (&'a Position, &'a Note);
    type IntoIter = std::collections::btree_map::Iter<'a, Position, Note>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl From<BTreeMap<Position, Note>> for Notes {
    fn from(map: BTreeMap<Position, Note>) -> Self {
        Notes(map)
    }
}

impl FromIterator<(Position, Note)> for Notes {
    fn from_iter<T: IntoIterator<Item = (Position, Note)>>(iter: T) -> Self {
        Notes(iter.into_iter().collect())
    }
}

impl<C> FromIterator<(Tick, C)> for Notes
where
    C: IntoIterator<Item = Tone>,
{
    fn from_iter<T: IntoIterator<Item = (Tick, C)>>(iter: T) -> Self {
        let ticked = iter.into_iter().sorted_by_key(|(tick, _)| *tick);
        let notes = ticked.flat_map(|(tick, tones)| {
            let indexed = tones.into_iter().enumerate();
            indexed.map(move |(idx, tone)| (Position::new(tick, idx as Index), Note::from(tone)))
        });
        notes.collect()
    }
}
