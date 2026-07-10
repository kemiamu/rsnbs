//! Generate Minecraft litematic projections from NBS songs.

use itertools::iproduct;
use mcdata::{GenericBlockState, util::BlockPos};
use rustmatica::{Litematic, Region};
use std::borrow::Cow;

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

// helper
//
// =============================================================================

/// A chain block (smooth stone) used for structural support.
pub fn chain_block() -> GenericBlockState {
    GenericBlockState {
        name: "minecraft:smooth_stone".into(),
        properties: Default::default(),
    }
}

/// A redstone wire block with all-side connections.
pub fn redstone_wire() -> GenericBlockState {
    use std::collections::HashMap;
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
