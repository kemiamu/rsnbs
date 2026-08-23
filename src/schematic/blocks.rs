//! Block state projections for notes and instruments.

use crate::note::{ImitateInstrument, Instrument, Tone};
use mcdata::GenericBlockState;
use std::{borrow::Cow, collections::HashMap};

// Tone block states
//
// ++++++++++++============++++++++++++============++++++++++++============

impl Tone {
    /// returns the minecraft note block block state for this tone.
    pub fn note_block_state(&self) -> Option<GenericBlockState> {
        let note = self.key.minecraft_note()?;
        let instr = self.instrument.note_property();
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

    /// returns the block under the note block for this instrument's sound.
    pub fn instrument_block_state(&self) -> Option<GenericBlockState> {
        if !self.is_valid() || matches!(self.instrument, Instrument::Imitate(_)) {
            return None;
        }
        let block = self.instrument.block_resource().unwrap();
        let properties = match self.instrument {
            Instrument::Banjo => HashMap::from([("axis".into(), "y".into())]),
            _ => HashMap::new(),
        };
        Some(GenericBlockState {
            name: block.into(),
            properties,
        })
    }

    /// returns the mob head block for this tone, if it is a mob head instrument.
    pub fn head_block_state(&self) -> Option<GenericBlockState> {
        if !self.is_valid() || !matches!(self.instrument, Instrument::Imitate(_)) {
            return None;
        }
        let block = self.instrument.block_resource().unwrap();
        Some(GenericBlockState {
            name: block.into(),
            properties: HashMap::new(),
        })
    }
}

// Instrument block states
//
// ++++++++++++============++++++++++++============++++++++++++============

impl Instrument {
    /// returns the minecraft instrument property string for note block state.
    pub fn note_property(&self) -> &'static str {
        match self {
            Self::Harp => "harp",
            Self::DoubleBass => "bass",
            Self::BassDrum => "basedrum",
            Self::SnareDrum => "snare",
            Self::Click => "hat",
            Self::Guitar => "guitar",
            Self::Flute => "flute",
            Self::Bell => "bell",
            Self::Chime => "chime",
            Self::Xylophone => "xylophone",
            Self::IronXylophone => "iron_xylophone",
            Self::CowBell => "cow_bell",
            Self::Didgeridoo => "didgeridoo",
            Self::Bit => "bit",
            Self::Banjo => "banjo",
            Self::Pling => "pling",
            Self::Trumpet => "trumpet",
            Self::TrumpetExposed => "trumpet_exposed",
            Self::TrumpetWeathered => "trumpet_weathered",
            Self::TrumpetOxidized => "trumpet_oxidized",
            Self::Imitate(instrument) => instrument.note_property(),
            Self::Custom(_) => "custom",
        }
    }

    /// returns the block resource name for this instrument.
    pub fn block_resource(&self) -> Option<&'static str> {
        Some(match self {
            Self::Harp => "minecraft:dirt",
            Self::DoubleBass => "minecraft:oak_planks",
            Self::BassDrum => "minecraft:stone",
            Self::SnareDrum => "minecraft:sand",
            Self::Click => "minecraft:glass",
            Self::Guitar => "minecraft:white_wool",
            Self::Flute => "minecraft:clay",
            Self::Bell => "minecraft:gold_block",
            Self::Chime => "minecraft:packed_ice",
            Self::Xylophone => "minecraft:bone_block",
            Self::IronXylophone => "minecraft:iron_block",
            Self::CowBell => "minecraft:soul_sand",
            Self::Didgeridoo => "minecraft:pumpkin",
            Self::Bit => "minecraft:emerald_block",
            Self::Banjo => "minecraft:hay_block",
            Self::Pling => "minecraft:glowstone",
            Self::Trumpet => "minecraft:waxed_copper_block",
            Self::TrumpetExposed => "minecraft:waxed_exposed_copper",
            Self::TrumpetWeathered => "minecraft:waxed_weathered_copper",
            Self::TrumpetOxidized => "minecraft:waxed_oxidized_copper",
            Self::Imitate(instrument) => instrument.block_resource(),
            Self::Custom(_) => return None,
        })
    }

    /// returns the block under the note block for this instrument's sound.
    pub fn instrument_block(&self) -> Option<GenericBlockState> {
        if matches!(self, Self::Imitate(_)) {
            return None;
        }
        let block = self.block_resource()?;
        Some(GenericBlockState {
            name: Cow::Borrowed(block),
            properties: HashMap::new(),
        })
    }

    /// returns the mob head block for this instrument, if it is a mob head instrument.
    pub fn head_block(&self) -> Option<GenericBlockState> {
        if !matches!(self, Self::Imitate(_)) {
            return None;
        }
        let block = self.block_resource()?;
        Some(GenericBlockState {
            name: block.into(),
            properties: HashMap::new(),
        })
    }
}

// ImitateInstrument block states
//
// ++++++++++++============++++++++++++============++++++++++++============

impl ImitateInstrument {
    /// returns the minecraft instrument property string for note block state.
    pub fn note_property(self) -> &'static str {
        match self {
            Self::Creeper => "creeper",
            Self::Skeleton => "skeleton",
            Self::Dragon => "ender_dragon",
            Self::WitherSkeleton => "wither_skeleton",
            Self::Piglin => "piglin",
            Self::Zombie => "zombie",
            Self::CustomHead => "custom_head",
        }
    }

    /// returns the block resource name for this mob head.
    pub fn block_resource(self) -> &'static str {
        match self {
            Self::Creeper => "minecraft:creeper_head",
            Self::Skeleton => "minecraft:skeleton_skull",
            Self::Dragon => "minecraft:dragon_head",
            Self::WitherSkeleton => "minecraft:wither_skeleton_skull",
            Self::Piglin => "minecraft:piglin_head",
            Self::Zombie => "minecraft:zombie_head",
            Self::CustomHead => "minecraft:player_head",
        }
    }
}

// Block state helpers
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Note block, or fallback on None.
pub fn note_block<T: AsRef<Tone>>(
    note: Option<T>,
    fallback: fn() -> GenericBlockState,
) -> GenericBlockState {
    note.and_then(|t| t.as_ref().note_block_state())
        .unwrap_or_else(fallback)
}

pub fn inst_block<T: AsRef<Tone>>(
    note: Option<T>,
    fallback: fn() -> GenericBlockState,
) -> GenericBlockState {
    note.and_then(|t| t.as_ref().instrument_block_state())
        .unwrap_or_else(fallback)
}

pub fn chain_block() -> GenericBlockState {
    GenericBlockState {
        name: "minecraft:smooth_stone".into(),
        properties: Default::default(),
    }
}

pub fn redstone_wire() -> GenericBlockState {
    let properties = HashMap::from([
        ("power".into(), "0".into()),
        ("north".into(), "side".into()),
        ("south".into(), "side".into()),
        ("east".into(), "side".into()),
        ("west".into(), "side".into()),
    ]);
    GenericBlockState {
        name: "minecraft:redstone_wire".into(),
        properties,
    }
}

/// Redstone wire with explicit connection states
/// (east/north/south/west: none|side|up) and signal strength.
///
/// These are the post-update states a placed wire settles into.
pub fn wire_state(
    east: &'static str,
    north: &'static str,
    south: &'static str,
    west: &'static str,
    power: &'static str,
) -> GenericBlockState {
    GenericBlockState {
        name: "minecraft:redstone_wire".into(),
        properties: HashMap::from([
            ("power".into(), power.into()),
            ("north".into(), north.into()),
            ("south".into(), south.into()),
            ("east".into(), east.into()),
            ("west".into(), west.into()),
        ]),
    }
}

/// Repeater block with delay, facing, and powered state.
pub fn repeater(
    delay: impl Into<Cow<'static, str>>,
    facing: impl Into<Cow<'static, str>>,
    powered: bool,
) -> GenericBlockState {
    let powered = if powered { "true" } else { "false" };
    GenericBlockState {
        name: "minecraft:repeater".into(),
        properties: HashMap::from([
            ("delay".into(), delay.into()),
            ("facing".into(), facing.into()),
            ("locked".into(), "false".into()),
            ("powered".into(), powered.into()),
        ]),
    }
}

/// Observer block with facing.
pub fn observer(facing: impl Into<Cow<'static, str>>) -> GenericBlockState {
    GenericBlockState {
        name: "minecraft:observer".into(),
        properties: HashMap::from([
            ("facing".into(), facing.into()),
            ("powered".into(), "false".into()),
        ]),
    }
}

/// Sticky piston block, not extended.
pub fn sticky_piston<T: Into<Cow<'static, str>>>(facing: T) -> GenericBlockState {
    GenericBlockState {
        name: "minecraft:sticky_piston".into(),
        properties: HashMap::from([
            ("facing".into(), facing.into()),
            ("extended".into(), "false".into()),
        ]),
    }
}

/// Redstone block.
pub fn redstone_block() -> GenericBlockState {
    GenericBlockState {
        name: "minecraft:redstone_block".into(),
        properties: Default::default(),
    }
}

/// Redstone torch with lit state and optional facing.
pub fn redstone_torch<T: Into<Cow<'static, str>>>(
    facing: Option<T>,
    lit: bool,
) -> GenericBlockState {
    let lit = if lit { "true" } else { "false" };
    let name = match facing.is_some() {
        true => "minecraft:redstone_wall_torch".into(),
        false => "minecraft:redstone_torch".into(),
    };
    let properties = match facing {
        Some(f) => From::from([("lit".into(), lit.into()), ("facing".into(), f.into())]),
        None => From::from([("lit".into(), lit.into())]),
    };
    GenericBlockState { name, properties }
}
