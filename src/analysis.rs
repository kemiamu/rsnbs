//! Translation analysis of the tick–tone plane.
//!
//! The theoretical core of the formal reduction `M = K (+) S + R`: base
//! types and abstractions live here, while decomposition algorithms live
//! in the [`reuse`] submodule.

use crate::note::{Notes, Tone};
use crate::types::{Tick, TickAnchor};
use counter::Counter;
use itertools::{Itertools, iproduct};
use std::collections::BTreeSet;
use std::iter::repeat;
use std::num::NonZero;
use std::ops::{BitAnd, Deref, DerefMut};

pub mod reuse;

// TpPlane
//
// ++++++++++++============++++++++++++============++++++++++++============

/// A point in the TP (tick-tone) plane.
pub type Point = (Tick, Tone);

/// TP (tick-tone) plane multiset.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TpPlane(Counter<Point>);

impl TpPlane {
    /// Expand into an iterator of individual `(Tick, Tone)` points.
    pub fn into_points(self) -> impl Iterator<Item = Point> {
        let TpPlane(inner) = self;
        inner
            .into_iter()
            .flat_map(|(point, count)| repeat(point).take(count))
    }
}

impl<K: TickAnchor, V: Into<Tone>> FromIterator<(K, V)> for TpPlane {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        let inner = iter
            .into_iter()
            .map(|(k, v)| (k.into_tick(), v.into()))
            .collect();
        Self(inner)
    }
}

impl From<TpPlane> for Notes<Tick, Vec<Tone>> {
    fn from(plane: TpPlane) -> Self {
        let mut by_tick = plane.into_points().into_group_map();
        for tones in by_tick.values_mut() {
            tones.sort();
        }
        Self::from_iter(by_tick)
    }
}

impl Deref for TpPlane {
    type Target = Counter<Point>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for TpPlane {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

// Translation Equivalence Class
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Translation Equivalence Class (TEC)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransEqClass {
    /// Offsets defining the translation pattern.
    offsets: BTreeSet<NonZero<Tick>>,
    /// Points (multiset) that this TEC operates on.
    points: TpPlane,
}

impl TransEqClass {
    pub fn new(offsets: BTreeSet<NonZero<Tick>>, points: TpPlane) -> Self {
        Self { offsets, points }
    }

    /// The offsets that define this translation pattern.
    pub fn offsets(&self) -> &BTreeSet<NonZero<Tick>> {
        &self.offsets
    }

    /// The anchor points common to all offsets.
    pub fn points(&self) -> &TpPlane {
        &self.points
    }

    /// Consume the TEC and return `(offsets, pruned_kernel)`.
    pub fn into_pruned(self) -> (BTreeSet<NonZero<Tick>>, TpPlane) {
        let offsets = self.offsets;
        let mut points = self.points;
        let indexes: Vec<Point> = points.keys().copied().sorted().collect();

        for (point @ (tick, tone), scatter) in iproduct!(indexes, offsets.iter()) {
            let shifted = &(tick + scatter.get(), tone);
            let anchor_mult = points[&point];
            let entry = points.entry(*shifted);
            entry.and_modify(|mult| *mult -= anchor_mult.min(*mult));
        }
        (offsets, points)
    }
}

impl BitAnd for TransEqClass {
    fn bitand(self, rhs: Self) -> Self {
        let offsets = &self.offsets | &rhs.offsets;
        let points = TpPlane(self.points.0 & rhs.points.0);
        Self { offsets, points }
    }
    type Output = Self;
}

impl<const N: usize> From<([NonZero<Tick>; N], TpPlane)> for TransEqClass {
    fn from((offsets, points): ([NonZero<Tick>; N], TpPlane)) -> Self {
        let offsets = BTreeSet::from(offsets);
        Self { offsets, points }
    }
}
