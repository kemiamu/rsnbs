//! Generate Minecraft litematic projections from NBS songs.

use crate::Note;
use itertools::iproduct;
use mcdata::{GenericBlockState, util::BlockPos};
use rustmatica::{Litematic, Region};
use std::borrow::Cow;
use std::collections::HashMap;

/// Output a [`Layout`] as a litematic file.
///
/// Example: `SchematicBuilder(layout).build("Song", "Me")`
pub struct SchematicBuilder<L: Layout>(pub L);

impl<L: Layout> SchematicBuilder<L> {
    /// Iterate every position in the layout's bounding box and produce a litematic.
    pub fn build(
        self,
        description: impl Into<Cow<'static, str>>,
        author: impl Into<Cow<'static, str>>,
    ) -> Litematic {
        let SchematicBuilder(layout) = self;
        let size = layout.size();
        let mut region: Region<GenericBlockState> =
            Region::new("Note Block Track Schematic", BlockPos::new(0, 0, 0), size);

        for (y, z, x) in iproduct!(0..size.y, 0..size.z, 0..size.x) {
            let pos = BlockPos::new(x, y, z);
            region.set_block(pos, layout.get_block(pos));
        }

        region.as_litematic(description, author)
    }
}

/// A queryable projection layout.
pub trait Layout {
    /// Total size of the bounding box.
    fn size(&self) -> BlockPos;
    /// Block at the given world position.
    fn get_block(&self, pos: BlockPos) -> GenericBlockState;
}

// Helpers
//
// ++++++++++++============++++++++++++============++++++++++++============

/// A chain block (smooth stone) used for structural support.
pub fn chain_block() -> GenericBlockState {
    GenericBlockState {
        name: "minecraft:smooth_stone".into(),
        properties: Default::default(),
    }
}

/// Floor block.
pub fn floor_block() -> GenericBlockState {
    GenericBlockState {
        name: "minecraft:white_concrete".into(),
        properties: Default::default(),
    }
}

/// A redstone wire block with all-side connections.
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

/// Repeater block with delay and facing.
pub fn repeater(
    delay: impl Into<Cow<'static, str>>,
    facing: impl Into<Cow<'static, str>>,
) -> GenericBlockState {
    GenericBlockState {
        name: "minecraft:repeater".into(),
        properties: HashMap::from([
            ("delay".into(), delay.into()),
            ("facing".into(), facing.into()),
            ("locked".into(), "false".into()),
            ("powered".into(), "false".into()),
        ]),
    }
}

/// Note block, or fallback on None.
pub fn note_block(note: Option<&Note>, fallback: fn() -> GenericBlockState) -> GenericBlockState {
    note.as_ref()
        .and_then(|n| n.note_block_state())
        .unwrap_or_else(fallback)
}

/// Instrument block, or fallback on None.
pub fn instrument_block(
    note: Option<&Note>,
    fallback: fn() -> GenericBlockState,
) -> GenericBlockState {
    note.as_ref()
        .and_then(|n| n.instrument.instrument_block())
        .unwrap_or_else(fallback)
}

/// Sticky piston block, not extended.
pub fn sticky_piston(facing: impl Into<Cow<'static, str>>) -> GenericBlockState {
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
