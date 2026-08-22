use crate::types::{Panning, Position, Volume};
use std::collections::BTreeMap;
use std::fmt::{self, Display, Formatter};
use std::ops::{Deref, DerefMut};

// notes collection
//
// ++++++++++++============++++++++++++============++++++++++++============

/// ordered note set, guarantees position order for nbs serialization.
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct Notes<Anchor = Position, Event = Note>(BTreeMap<Anchor, Event>);

impl<A, E> Default for Notes<A, E> {
    fn default() -> Self {
        Notes(Default::default())
    }
}

impl<A, E> From<BTreeMap<A, E>> for Notes<A, E> {
    fn from(map: BTreeMap<A, E>) -> Self {
        Notes(map)
    }
}

impl<A: Ord, E> FromIterator<(A, E)> for Notes<A, E> {
    fn from_iter<I: IntoIterator<Item = (A, E)>>(iter: I) -> Self {
        Notes(BTreeMap::from_iter(iter))
    }
}

impl<A, E> Deref for Notes<A, E> {
    type Target = BTreeMap<A, E>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<A, E> DerefMut for Notes<A, E> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<A, E> IntoIterator for Notes<A, E> {
    type Item = (A, E);
    type IntoIter = std::collections::btree_map::IntoIter<A, E>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a, A, E> IntoIterator for &'a Notes<A, E> {
    type Item = (&'a A, &'a E);
    type IntoIter = std::collections::btree_map::Iter<'a, A, E>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

// note
//
// ++++++++++++============++++++++++++============++++++++++++============

/// a single note with timing, instrument, and modulation data.
#[derive(Debug, Default, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Note {
    pub(super) tone: Tone,
    pub(super) velocity: Volume,
    pub(super) panning: Panning,
    pub(super) pitch: i16,
}

impl Note {
    /// creates a note from any value that can convert into one.
    pub fn new<T: Into<Self>>(value: T) -> Self {
        value.into()
    }

    /// returns the tone as a pair of instrument and key.
    pub fn tone(&self) -> Tone {
        self.tone
    }

    /// returns the modulation parameters of the note.
    pub fn modulation(&self) -> Modulation {
        Modulation {
            velocity: self.velocity,
            panning: self.panning,
            pitch: self.pitch,
        }
    }

    /// replaces the note's instrument, keeping its key and modulation.
    pub(crate) fn set_instrument(&mut self, instrument: Instrument) {
        self.tone.instrument = instrument;
    }
}

impl AsRef<Tone> for Note {
    fn as_ref(&self) -> &Tone {
        &self.tone
    }
}

impl From<Tone> for Note {
    fn from(tone: Tone) -> Self {
        Self {
            tone,
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

    /// whether this tone is renderable: built-in instrument with a minecraft note.
    pub(crate) fn is_valid(&self) -> bool {
        !matches!(self.instrument, Instrument::Custom(_)) && self.key.minecraft_note().is_some()
    }
}

impl From<Note> for Tone {
    fn from(note: Note) -> Self {
        note.tone
    }
}

impl AsRef<Tone> for Tone {
    fn as_ref(&self) -> &Tone {
        self
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
    // Mob head instruments. Reserved by rsnbs: not part of the NBS file
    // format, used for schematic rendering of mob head note blocks. They
    // are not encodable in files and fall back to Harp when saved.
    Imitate(ImitateInstrument),
    // Custom instruments, identified by their slot index in the song's
    // custom instrument list (the NBS instrument byte minus the song's
    // first custom instrument index).
    Custom(u8),
}

/// mob head sounds, reserved by rsnbs for schematic rendering.
/// these are not NBS instruments and cannot be saved to files.
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
    /// Vanilla instruments in NBS serialization order; the array position is
    /// the NBS instrument index. The four v6 trumpets occupy indexes 16-19.
    /// Note: byte encoding/decoding relative to the first custom instrument
    /// index lives in codec.rs.
    pub(crate) const NBS_INDEX: [Instrument; 20] = [
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
    ];

    /// The number of vanilla instruments in the newest NBS version.
    pub(crate) fn vanilla_count() -> u8 {
        Self::NBS_INDEX.len() as u8
    }

    /// Returns the fixed table index of a vanilla instrument, if this is one.
    /// Vanilla instruments occupy indexes 0..20 (including the v6 trumpets).
    pub fn vanilla_index(self) -> Option<u8> {
        Self::NBS_INDEX
            .iter()
            .position(|&inst| inst == self)
            .map(|index| index as u8)
    }

    /// Returns the canonical OpenNBS name and sound file of a vanilla
    /// instrument, used as the generic fallback when such an instrument must
    /// be stored as a custom instrument in an older NBS version.
    pub fn nbs_definition(self) -> Option<(&'static str, &'static str)> {
        use Instrument::*;
        Some(match self {
            Harp => ("Harp", "harp.ogg"),
            DoubleBass => ("Double Bass", "dbass.ogg"),
            BassDrum => ("Bass Drum", "bdrum.ogg"),
            SnareDrum => ("Snare Drum", "sdrum.ogg"),
            Click => ("Click", "click.ogg"),
            Guitar => ("Guitar", "guitar.ogg"),
            Flute => ("Flute", "flute.ogg"),
            Bell => ("Bell", "bell.ogg"),
            Chime => ("Chime", "icechime.ogg"),
            Xylophone => ("Xylophone", "xylobone.ogg"),
            IronXylophone => ("Iron Xylophone", "iron_xylophone.ogg"),
            CowBell => ("Cow Bell", "cow_bell.ogg"),
            Didgeridoo => ("Didgeridoo", "didgeridoo.ogg"),
            Bit => ("Bit", "bit.ogg"),
            Banjo => ("Banjo", "banjo.ogg"),
            Pling => ("Pling", "pling.ogg"),
            Trumpet => ("Trumpet", "trumpet.ogg"),
            TrumpetExposed => ("Exposed Trumpet", "trumpet_exposed.ogg"),
            TrumpetWeathered => ("Weathered Trumpet", "trumpet_weathered.ogg"),
            TrumpetOxidized => ("Oxidized Trumpet", "trumpet_oxidized.ogg"),
            _ => return None,
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
