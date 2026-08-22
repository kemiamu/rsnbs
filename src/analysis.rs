//! Translation analysis of the tick-tone plane.
//!
//! The theoretical core of the formal reduction `M = K (+) S + R`: base
//! types and abstractions live here, while decomposition algorithms live
//! in the [`reuse`] submodule.

use crate::note::Notes;
use crate::types::{Tick, TimeAnchor};
use counter::Counter;
use itertools::{Itertools, iproduct};
use std::collections::BTreeSet;
use std::fmt::Debug;
use std::hash::Hash;
use std::iter::repeat;
use std::num::NonZero;
use std::ops::{BitAnd, Deref, DerefMut};

pub mod reuse;

// Event
//
// ++++++++++++============++++++++++++============++++++++++++============

/// The plane's second axis: an event occurring at a tick.
///
/// In the design document this is a tone, which may be any enum type;
/// `note::Notes` uses the same word for its value type. `Event` is the
/// minimal capability set the plane and TEC machinery need from it.
pub trait Event: Hash + Eq + Ord + Clone + Debug {}
impl<T: Hash + Eq + Ord + Clone + Debug> Event for T {}

// TpPlane
//
// ++++++++++++============++++++++++++============++++++++++++============

/// A point in the TP (tick-tone) plane.
pub type Point<E> = (Tick, E);

/// TP (tick-tone) plane multiset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TpPlane<E: Event>(Counter<Point<E>>);

impl<E: Event> Default for TpPlane<E> {
    fn default() -> Self {
        TpPlane(Counter::new())
    }
}

impl<E: Event> TpPlane<E> {
    /// Expand into an iterator of individual `(Tick, Event)` points.
    pub fn into_points(self) -> impl Iterator<Item = Point<E>> {
        let TpPlane(inner) = self;
        inner
            .into_iter()
            .flat_map(|(point, count)| repeat(point).take(count))
    }
}

impl<K: TimeAnchor, V: Into<E>, E: Event> FromIterator<(K, V)> for TpPlane<E> {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        let inner = iter.into_iter().map(|(k, v)| (k.into_tick(), v.into()));
        Self(inner.collect())
    }
}

impl<E: Event> From<TpPlane<E>> for Notes<Tick, Vec<E>> {
    fn from(plane: TpPlane<E>) -> Self {
        let by_tick = plane.into_points().into_group_map();
        let notes = by_tick.into_iter().map(|(tick, mut tones)| {
            tones.sort_unstable();
            (tick, tones)
        });
        notes.collect()
    }
}

impl<E: Event> Deref for TpPlane<E> {
    type Target = Counter<Point<E>>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<E: Event> DerefMut for TpPlane<E> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

// Translation Equivalence Class
//
// ++++++++++++============++++++++++++============++++++++++++============

/// Translation Equivalence Class (TEC)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransEqClass<E: Event> {
    /// Translation offsets (scatter, ascending, excluding 0: zero is implied).
    pub scatter: BTreeSet<NonZero<Tick>>,
    /// Points (multiset) that this TEC operates on.
    pub kernel: TpPlane<E>,
}

impl<E: Event> TransEqClass<E> {
    pub fn new(scatter: BTreeSet<NonZero<Tick>>, kernel: TpPlane<E>) -> Self {
        Self { scatter, kernel }
    }
}

impl<E: Event> BitAnd for TransEqClass<E> {
    fn bitand(self, rhs: Self) -> Self {
        let scatter = &self.scatter | &rhs.scatter;
        let kernel = TpPlane(self.kernel.0 & rhs.kernel.0);
        Self { scatter, kernel }
    }
    type Output = Self;
}

// Bounded TEC
//
// ++++++++++++============++++++++++++============++++++++++++============

/// A TEC whose kernel expansion stays within its points:
/// `kernel (+) scatter <= points`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundedTec<E: Event>(TransEqClass<E>);

impl<E: Event> BoundedTec<E> {
    /// Deducts each point's covered multiplicity from its shifted copies,
    /// keeping the kernel expansion within the TEC's points.
    pub fn new(mut tec: TransEqClass<E>) -> Self {
        let indexes: Vec<Point<E>> = tec.kernel.keys().cloned().sorted().collect();
        for (point, scatter_offset) in iproduct!(indexes, tec.scatter.iter()) {
            let anchor_mult = tec.kernel[&point];
            let (tick, tone) = point;
            let shifted = (tick + scatter_offset.get(), tone);
            let entry = tec.kernel.entry(shifted);
            entry.and_modify(|mult| *mult -= anchor_mult.min(*mult));
        }
        BoundedTec(tec)
    }

    /// Unwrap into the underlying (already bounded) TEC.
    pub fn into_inner(self) -> TransEqClass<E> {
        self.0
    }
}

impl<E: Event> Deref for BoundedTec<E> {
    type Target = TransEqClass<E>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
